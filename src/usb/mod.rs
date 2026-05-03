pub mod mass_storage;
pub mod xhci;

use alloc::boxed::Box;
use spin::Mutex;

// ---------------------------------------------------------------------------
// BlockDevice trait
// ---------------------------------------------------------------------------

/// A block device that can read 512-byte sectors.
pub trait BlockDevice: Send {
    fn read_sectors(&mut self, lba: u64, count: u16) -> Option<alloc::vec::Vec<u8>>;
    fn write_sectors(&mut self, lba: u64, data: &[u8]) -> Option<()>;
    fn sector_count(&self) -> u64;
}

// ---------------------------------------------------------------------------
// Global XHCI controller
// ---------------------------------------------------------------------------

/// The global XHCI host controller.  Initialised once in `kernel_main` and
/// kept alive for the lifetime of the kernel so that hot-plug rescans and
/// ongoing FAT32 I/O can reach it.
pub static USB_XHCI: Mutex<Option<xhci::Xhci>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Index-based block device wrapper
// ---------------------------------------------------------------------------

/// A `BlockDevice` backed by a specific device index in `USB_XHCI`.
///
/// Storing only the index means many `XhciBlockDevice`s can coexist and each
/// transparently delegates to the shared, global XHCI controller.
pub struct XhciBlockDevice {
    pub dev_idx: usize,
}

impl BlockDevice for XhciBlockDevice {
    fn read_sectors(&mut self, lba: u64, count: u16) -> Option<alloc::vec::Vec<u8>> {
        USB_XHCI
            .lock()
            .as_mut()?
            .read_sectors_dev(self.dev_idx, lba, count)
    }

    fn write_sectors(&mut self, lba: u64, data: &[u8]) -> Option<()> {
        USB_XHCI
            .lock()
            .as_mut()?
            .write_sectors_dev(self.dev_idx, lba, data)
    }

    fn sector_count(&self) -> u64 {
        USB_XHCI
            .lock()
            .as_ref()
            .and_then(|x| x.devices.get(self.dev_idx))
            .map(|d| d.block_count)
            .unwrap_or(0)
    }
}

/// A `BlockDevice` view into a partition of another block device.
pub struct PartitionBlockDevice {
    inner: Box<dyn BlockDevice>,
    start_lba: u64,
    sector_count: u64,
}

impl PartitionBlockDevice {
    pub fn new(inner: Box<dyn BlockDevice>, start_lba: u64, sector_count: u64) -> Self {
        Self {
            inner,
            start_lba,
            sector_count,
        }
    }
}

impl BlockDevice for PartitionBlockDevice {
    fn read_sectors(&mut self, lba: u64, count: u16) -> Option<alloc::vec::Vec<u8>> {
        let end = lba.checked_add(count as u64)?;
        if end > self.sector_count {
            return None;
        }
        self.inner
            .read_sectors(self.start_lba.checked_add(lba)?, count)
    }

    fn write_sectors(&mut self, lba: u64, data: &[u8]) -> Option<()> {
        if !data.len().is_multiple_of(512) {
            return None;
        }
        let count = (data.len() / 512) as u64;
        let end = lba.checked_add(count)?;
        if end > self.sector_count {
            return None;
        }
        self.inner
            .write_sectors(self.start_lba.checked_add(lba)?, data)
    }

    fn sector_count(&self) -> u64 {
        self.sector_count
    }
}

#[derive(Clone, Copy)]
pub struct PartitionInfo {
    pub start_lba: u64,
    pub sector_count: u64,
}

fn read_u32_le(buf: &[u8], off: usize) -> Option<u32> {
    if off.checked_add(4)? > buf.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        buf[off],
        buf[off + 1],
        buf[off + 2],
        buf[off + 3],
    ]))
}

fn read_u64_le(buf: &[u8], off: usize) -> Option<u64> {
    if off.checked_add(8)? > buf.len() {
        return None;
    }
    Some(u64::from_le_bytes([
        buf[off],
        buf[off + 1],
        buf[off + 2],
        buf[off + 3],
        buf[off + 4],
        buf[off + 5],
        buf[off + 6],
        buf[off + 7],
    ]))
}

/// Public wrapper around `gpt_partitions_for_device` for use in shell mount command.
pub fn gpt_partitions_for_device_pub(dev_idx: usize) -> alloc::vec::Vec<PartitionInfo> {
    gpt_partitions_for_device(dev_idx)
}

fn gpt_partitions_for_device(dev_idx: usize) -> alloc::vec::Vec<PartitionInfo> {
    use alloc::vec::Vec;

    let mut dev = XhciBlockDevice { dev_idx };
    let hdr = match dev.read_sectors(1, 1) {
        Some(h) if h.len() >= 512 => h,
        _ => return Vec::new(),
    };
    if &hdr[..8] != b"EFI PART" {
        return Vec::new();
    }

    let entries_lba = match read_u64_le(&hdr, 72) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let mut entry_count = match read_u32_le(&hdr, 80) {
        Some(v) => v as usize,
        None => return Vec::new(),
    };
    let entry_size = match read_u32_le(&hdr, 84) {
        Some(v) => v as usize,
        None => return Vec::new(),
    };
    if entry_size < 128 {
        return Vec::new();
    }
    if entry_count > 256 {
        entry_count = 256;
    }

    let total_bytes = match entry_count.checked_mul(entry_size) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let sectors = total_bytes.div_ceil(512) as u16;
    let entries = match dev.read_sectors(entries_lba, sectors) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let mut out = Vec::new();
    for i in 0..entry_count {
        let off = i * entry_size;
        if off + entry_size > entries.len() {
            break;
        }
        // Unused entry = zero type GUID (first 16 bytes all zero).
        if entries[off..off + 16].iter().all(|b| *b == 0) {
            continue;
        }
        let first = match read_u64_le(&entries, off + 32) {
            Some(v) => v,
            None => continue,
        };
        let last = match read_u64_le(&entries, off + 40) {
            Some(v) => v,
            None => continue,
        };
        if last < first {
            continue;
        }
        out.push(PartitionInfo {
            start_lba: first,
            sector_count: (last - first) + 1,
        });
    }

    out
}

/// Mount partition 2 of USB device 0 as the root filesystem (`/`) if possible.
pub fn mount_boot_storage_root() -> bool {
    let partitions = gpt_partitions_for_device(0);
    if partitions.len() < 2 {
        return false;
    }
    let p2 = partitions[1];
    let block_dev: Box<dyn BlockDevice> = Box::new(PartitionBlockDevice::new(
        Box::new(XhciBlockDevice { dev_idx: 0 }),
        p2.start_lba,
        p2.sector_count,
    ));
    let Some(fat32) = crate::fs::fat32::Fat32Fs::new(block_dev) else {
        return false;
    };
    let mut vfs = crate::vfs::VFS.lock();
    let Some(vfs) = vfs.as_mut() else {
        return false;
    };
    vfs.set_root(
        Box::new(crate::vfs::Fat32Mount(fat32)),
        "fat32 RUSTOS_ROOT persistent",
    );
    crate::println!("[usb] mounted device0 partition2 FAT32 as root filesystem '/'");
    true
}

// ---------------------------------------------------------------------------
// Mount helpers (called from kernel_main and from the `usbscan` shell command)
// ---------------------------------------------------------------------------

/// Try to mount USB devices starting from `from_idx` as FAT32 volumes.
///
/// Device 0 → `/usb`, device 1 → `/usb1`, device 2 → `/usb2`, …
///
/// Safe to call while `USB_XHCI` is **not** locked; the function locks it
/// briefly only to read the device count, then releases it before doing any
/// FAT32 I/O (which re-acquires the lock per-sector).
pub fn mount_storage_devices(from_idx: usize) {
    let device_count = USB_XHCI
        .lock()
        .as_ref()
        .map(|x| x.devices.len())
        .unwrap_or(0);

    for dev_idx in from_idx..device_count {
        let mount_path = if dev_idx == 0 {
            alloc::string::String::from("/usb")
        } else {
            alloc::format!("/usb{}", dev_idx)
        };

        let mut mounted = false;

        // First try the whole device as a FAT32 volume.
        if let Some(fat32) = crate::fs::fat32::Fat32Fs::new(Box::new(XhciBlockDevice { dev_idx })) {
            let mut vfs = crate::vfs::VFS.lock();
            if let Some(vfs) = vfs.as_mut() {
                vfs.mount(&mount_path, Box::new(crate::vfs::Fat32Mount(fat32)));
                crate::println!("[usb] FAT32 volume mounted at {}", mount_path);
                mounted = true;
            }
        }

        // If not, try GPT partitions and mount the first FAT32 partition.
        if !mounted {
            for part in gpt_partitions_for_device(dev_idx) {
                let pdev: Box<dyn BlockDevice> = Box::new(PartitionBlockDevice::new(
                    Box::new(XhciBlockDevice { dev_idx }),
                    part.start_lba,
                    part.sector_count,
                ));
                if let Some(fat32) = crate::fs::fat32::Fat32Fs::new(pdev) {
                    let mut vfs = crate::vfs::VFS.lock();
                    if let Some(vfs) = vfs.as_mut() {
                        vfs.mount(&mount_path, Box::new(crate::vfs::Fat32Mount(fat32)));
                        crate::println!("[usb] FAT32 partition mounted at {}", mount_path);
                        mounted = true;
                        break;
                    }
                }
            }
        }

        if !mounted {
            crate::serial_println!("[usb] device {} has no FAT32 volume", dev_idx);
        }
    }
}

/// Scan all XHCI ports for newly connected devices, enumerate them, and
/// mount any FAT32 volumes found.
///
/// Returns the number of new storage devices that were successfully mounted.
/// Designed to be called from the `usbscan` shell command.
pub fn scan_and_mount() -> usize {
    // Phase 1: scan ports inside the XHCI lock, collect new device count.
    let before = USB_XHCI
        .lock()
        .as_ref()
        .map(|x| x.devices.len())
        .unwrap_or(0);
    {
        let mut xhci = USB_XHCI.lock();
        if let Some(x) = xhci.as_mut() {
            x.scan_new_ports();
        }
    } // USB_XHCI lock released here

    // Phase 2: mount any newly discovered devices (acquires USB_XHCI briefly
    // per-sector, never while VFS is held).
    let after = USB_XHCI
        .lock()
        .as_ref()
        .map(|x| x.devices.len())
        .unwrap_or(0);
    if after > before {
        mount_storage_devices(before);
    }
    after - before
}
