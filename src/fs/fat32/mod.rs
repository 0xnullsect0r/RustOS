//! FAT32 read-only filesystem driver.
//!
//! Supports:
//!  - FAT32 BPB (BIOS Parameter Block) parsing
//!  - Directory traversal (8.3 short names + LFN long file names)
//!  - Cluster chain following via the FAT
//!  - File and directory content reading
//!
//! Entry point: `Fat32Fs::new(block_dev)` where `block_dev` implements
//! `crate::usb::BlockDevice`.

use crate::usb::BlockDevice;
use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};

// ---------------------------------------------------------------------------
// BPB (BIOS Parameter Block) — first 512 bytes of the volume
// ---------------------------------------------------------------------------

#[repr(C, packed)]
#[allow(dead_code)]
struct Bpb {
    jump_boot: [u8; 3],
    oem_name: [u8; 8],
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    root_entry_count: u16, // 0 for FAT32
    total_sectors_16: u16, // 0 for FAT32
    media: u8,
    fat_size_16: u16, // 0 for FAT32
    sectors_per_track: u16,
    num_heads: u16,
    hidden_sectors: u32,
    total_sectors_32: u32,
    // FAT32 extended BPB
    fat_size_32: u32,
    ext_flags: u16,
    fs_version: u16,
    root_cluster: u32,
    fs_info: u16,
    backup_boot_sec: u16,
    _reserved: [u8; 12],
    drive_number: u8,
    _reserved2: u8,
    boot_signature: u8,
    volume_id: u32,
    volume_label: [u8; 11],
    fs_type: [u8; 8],
}

// ---------------------------------------------------------------------------
// Directory Entry (32 bytes)
// ---------------------------------------------------------------------------

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct DirEntry32 {
    name: [u8; 11],
    attributes: u8,
    _nt_res: u8,
    _crt_time_tenth: u8,
    _crt_time: u16,
    _crt_date: u16,
    _lst_acc_date: u16,
    fst_clus_hi: u16,
    _wrt_time: u16,
    _wrt_date: u16,
    fst_clus_lo: u16,
    file_size: u32,
}

const ATTR_READ_ONLY: u8 = 0x01;
const ATTR_HIDDEN: u8 = 0x02;
const ATTR_SYSTEM: u8 = 0x04;
const ATTR_VOLUME_ID: u8 = 0x08;
const ATTR_DIRECTORY: u8 = 0x10;
#[allow(dead_code)]
const ATTR_ARCHIVE: u8 = 0x20;
const ATTR_LONG_NAME: u8 = ATTR_READ_ONLY | ATTR_HIDDEN | ATTR_SYSTEM | ATTR_VOLUME_ID;

// ---------------------------------------------------------------------------
// LFN (Long File Name) entry
// ---------------------------------------------------------------------------

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct LfnEntry {
    order: u8,
    name1: [u16; 5],
    attributes: u8,
    lfn_type: u8,
    checksum: u8,
    name2: [u16; 6],
    fst_clus: u16,
    name3: [u16; 2],
}

// ---------------------------------------------------------------------------
// Public directory entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FatDirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u32,
    pub cluster: u32,
}

// ---------------------------------------------------------------------------
// FAT32 Filesystem
// ---------------------------------------------------------------------------

pub struct Fat32Fs {
    dev: Box<dyn BlockDevice>,
    bytes_per_sector: u32,
    secs_per_cluster: u32,
    fat_start_sector: u64,
    data_start_sector: u64,
    root_cluster: u32,
}

impl Fat32Fs {
    /// Mount a FAT32 filesystem from a block device.
    /// Reads sector 0 and parses the BPB.  Returns `None` if not FAT32.
    pub fn new(mut dev: Box<dyn BlockDevice>) -> Option<Self> {
        let boot = dev.read_sectors(0, 1)?;
        if boot.len() < 512 {
            return None;
        }
        let bpb = unsafe { &*(boot.as_ptr() as *const Bpb) };

        let bps = u16::from_le(bpb.bytes_per_sector) as u32;
        let spc = bpb.sectors_per_cluster as u32;
        let rsvd = u16::from_le(bpb.reserved_sectors) as u64;
        let nfat = bpb.num_fats as u64;
        let fat_sz = u32::from_le(bpb.fat_size_32) as u64;
        let root_clus = u32::from_le(bpb.root_cluster);
        let fat_start = rsvd;
        let data_start = rsvd + nfat * fat_sz;

        // Validate FAT32 signature
        if &bpb.fs_type[..5] != b"FAT32" {
            // Fallback: check if FAT size 16 is 0 (FAT32 indicator)
            if u16::from_le(bpb.fat_size_16) != 0 {
                return None;
            }
        }

        Some(Fat32Fs {
            dev,
            bytes_per_sector: bps,
            secs_per_cluster: spc,
            fat_start_sector: fat_start,
            data_start_sector: data_start,
            root_cluster: root_clus,
        })
    }

    // -----------------------------------------------------------------------
    // Cluster <-> sector conversion
    // -----------------------------------------------------------------------

    fn cluster_to_sector(&self, cluster: u32) -> u64 {
        self.data_start_sector + (cluster as u64 - 2) * self.secs_per_cluster as u64
    }

    fn sectors_per_cluster(&self) -> u16 {
        self.secs_per_cluster as u16
    }

    // -----------------------------------------------------------------------
    // FAT chain traversal
    // -----------------------------------------------------------------------

    fn next_cluster(&mut self, cluster: u32) -> Option<u32> {
        // Each FAT32 entry is 4 bytes
        let fat_entries_per_sector = self.bytes_per_sector / 4;
        let sector_off = cluster / fat_entries_per_sector;
        let entry_off = (cluster % fat_entries_per_sector) as usize;

        let sector = self.fat_start_sector + sector_off as u64;
        let data = self.dev.read_sectors(sector, 1)?;

        let entry = u32::from_le_bytes([
            data[entry_off * 4],
            data[entry_off * 4 + 1],
            data[entry_off * 4 + 2],
            data[entry_off * 4 + 3],
        ]) & 0x0FFF_FFFF;

        if entry >= 0x0FFF_FFF8 {
            None
        } else {
            Some(entry)
        }
    }

    fn cluster_chain(&mut self, start: u32) -> Vec<u32> {
        let mut chain = Vec::new();
        let mut cur = start;
        loop {
            if !(2..0x0FFF_FFF8).contains(&cur) {
                break;
            }
            chain.push(cur);
            match self.next_cluster(cur) {
                Some(next) if next >= 2 => cur = next,
                _ => break,
            }
            if chain.len() > 65536 {
                break;
            } // safety limit
        }
        chain
    }

    // -----------------------------------------------------------------------
    // Read all sectors of a cluster chain into a byte buffer
    // -----------------------------------------------------------------------

    fn read_chain(&mut self, start_cluster: u32) -> Vec<u8> {
        let chain = self.cluster_chain(start_cluster);
        let mut buf = Vec::new();
        for cluster in chain {
            let sector = self.cluster_to_sector(cluster);
            if let Some(data) = self.dev.read_sectors(sector, self.sectors_per_cluster()) {
                buf.extend_from_slice(&data);
            }
        }
        buf
    }

    // -----------------------------------------------------------------------
    // Directory listing
    // -----------------------------------------------------------------------

    pub fn list_dir(&mut self, cluster: u32) -> Vec<FatDirEntry> {
        let data = self.read_chain(cluster);
        let mut entries = Vec::new();
        let mut lfn_buf: Vec<(u8, Vec<u16>)> = Vec::new(); // (order, chars)

        let n = data.len() / 32;
        for i in 0..n {
            let off = i * 32;
            if off + 32 > data.len() {
                break;
            }
            let raw = &data[off..off + 32];
            let first = raw[0];

            if first == 0x00 {
                break;
            } // end of directory
            if first == 0xE5 {
                lfn_buf.clear();
                continue;
            } // deleted

            let attr = raw[11];

            if attr == ATTR_LONG_NAME {
                // LFN entry
                let lfn = unsafe { &*(raw.as_ptr() as *const LfnEntry) };
                let mut chars: Vec<u16> = Vec::new();
                let n1 = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(lfn.name1)) };
                let n2 = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(lfn.name2)) };
                let n3 = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(lfn.name3)) };
                for c in n1 {
                    if c != 0xFFFF && c != 0 {
                        chars.push(u16::from_le(c));
                    }
                }
                for c in n2 {
                    if c != 0xFFFF && c != 0 {
                        chars.push(u16::from_le(c));
                    }
                }
                for c in n3 {
                    if c != 0xFFFF && c != 0 {
                        chars.push(u16::from_le(c));
                    }
                }
                lfn_buf.push((lfn.order & 0x1F, chars));
                continue;
            }

            if attr & (ATTR_VOLUME_ID | ATTR_SYSTEM) != 0 {
                lfn_buf.clear();
                continue;
            }

            let dir_entry = unsafe { &*(raw.as_ptr() as *const DirEntry32) };
            let cluster_hi = u16::from_le(dir_entry.fst_clus_hi) as u32;
            let cluster_lo = u16::from_le(dir_entry.fst_clus_lo) as u32;
            let cluster = (cluster_hi << 16) | cluster_lo;
            let file_size = u32::from_le(dir_entry.file_size);
            let is_dir = attr & ATTR_DIRECTORY != 0;

            let name = if !lfn_buf.is_empty() {
                // Reassemble LFN: sort by order ascending, concatenate
                lfn_buf.sort_by_key(|(o, _)| *o);
                let chars: Vec<u16> = lfn_buf
                    .iter()
                    .flat_map(|(_, c)| c.iter().cloned())
                    .collect();
                String::from_utf16_lossy(&chars).to_string()
            } else {
                parse_83_name(&dir_entry.name)
            };

            lfn_buf.clear();

            if name == "." || name == ".." {
                continue;
            }

            entries.push(FatDirEntry {
                name,
                is_dir,
                size: file_size,
                cluster,
            });
        }
        entries
    }

    // -----------------------------------------------------------------------
    // Path resolution
    // -----------------------------------------------------------------------

    /// Resolve a path (e.g. "/dir/subdir/file.txt") to its directory entry.
    /// Returns `None` if not found.
    pub fn lookup(&mut self, path: &str) -> Option<FatDirEntry> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut cluster = self.root_cluster;

        for (i, part) in parts.iter().enumerate() {
            let entries = self.list_dir(cluster);
            let found = entries
                .into_iter()
                .find(|e| e.name.eq_ignore_ascii_case(part))?;
            if i == parts.len() - 1 {
                return Some(found);
            }
            if !found.is_dir {
                return None;
            }
            cluster = found.cluster;
        }
        // path was "/" → return a synthetic root entry
        Some(FatDirEntry {
            name: String::from("/"),
            is_dir: true,
            size: 0,
            cluster: self.root_cluster,
        })
    }

    /// List directory at `path`.
    pub fn list(&mut self, path: &str) -> Option<Vec<FatDirEntry>> {
        let cluster = if path == "/" || path.is_empty() {
            self.root_cluster
        } else {
            let e = self.lookup(path)?;
            if !e.is_dir {
                return None;
            }
            e.cluster
        };
        Some(self.list_dir(cluster))
    }

    /// Read file contents at `path`.
    pub fn read_file(&mut self, path: &str) -> Option<Vec<u8>> {
        let entry = self.lookup(path)?;
        if entry.is_dir {
            return None;
        }
        let mut data = self.read_chain(entry.cluster);
        data.truncate(entry.size as usize);
        Some(data)
    }
}

// ---------------------------------------------------------------------------
// 8.3 short name parsing
// ---------------------------------------------------------------------------

fn parse_83_name(raw: &[u8; 11]) -> String {
    let base: Vec<u8> = raw[..8]
        .iter()
        .copied()
        .take_while(|&b| b != b' ')
        .collect();
    let ext: Vec<u8> = raw[8..11]
        .iter()
        .copied()
        .take_while(|&b| b != b' ')
        .collect();
    let name = String::from_utf8_lossy(&base).trim().to_string();
    if ext.is_empty() {
        name
    } else {
        alloc::format!("{}.{}", name, String::from_utf8_lossy(&ext).trim())
    }
}
