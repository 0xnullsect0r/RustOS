//! File descriptor table for userspace processes.
//!
//! Provides a simple global table mapping file descriptor numbers to open
//! VFS entries.  FDs 0–2 are reserved for stdin/stdout/stderr and are never
//! stored here.  All other FDs are allocated starting from 3.

use alloc::{string::String, vec::Vec};
use spin::Mutex;

/// Maximum number of concurrently open file descriptors (excluding 0-2).
const MAX_OPEN: usize = 16;

/// An open file or directory handle.
pub enum FdEntry {
    /// A regular file: buffered content + current read position.
    File { data: Vec<u8>, pos: usize },
    /// A directory: path stored so getdents64 can enumerate it.
    Directory { path: String },
}

struct Table {
    slots: [Option<FdEntry>; MAX_OPEN],
}

impl Table {
    const fn new() -> Self {
        // Can't use array-init with non-Copy Option<FdEntry>, so we use
        // a manual approach via MaybeUninit-free const evaluation trick.
        Table {
            slots: [
                None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None, None,
            ],
        }
    }

    /// Allocate a slot and return the fd number (3+), or -1 if the table is full.
    fn alloc(&mut self, entry: FdEntry) -> i64 {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(entry);
                return (i + 3) as i64; // reserve 0-2 for stdin/stdout/stderr
            }
        }
        -1
    }

    /// Get a mutable reference to the entry for `fd`, if valid.
    fn get_mut(&mut self, fd: i64) -> Option<&mut FdEntry> {
        if fd < 3 {
            return None;
        }
        let idx = (fd - 3) as usize;
        self.slots.get_mut(idx)?.as_mut()
    }

    /// Close `fd`, returning true if it was open.
    fn close(&mut self, fd: i64) -> bool {
        if fd < 3 {
            return false;
        }
        let idx = (fd - 3) as usize;
        if let Some(slot) = self.slots.get_mut(idx) {
            if slot.is_some() {
                *slot = None;
                return true;
            }
        }
        false
    }

    /// Close all open fds (call on process exit).
    fn close_all(&mut self) {
        for slot in self.slots.iter_mut() {
            *slot = None;
        }
    }
}

static FD_TABLE: Mutex<Table> = Mutex::new(Table::new());

/// Open a path on the VFS, returning a new fd on success or a negative error code.
pub fn open(path: &str) -> i64 {
    // Verify VFS is initialised before proceeding.
    {
        let vfs = crate::vfs::VFS.lock();
        if vfs.as_ref().is_none() {
            return -5; // EIO
        }
    }
    let mut vfs = crate::vfs::VFS.lock();
    let vfs_mut = match vfs.as_mut() {
        Some(v) => v,
        None => return -5,
    };

    if vfs_mut.is_dir(path) {
        let entry = FdEntry::Directory {
            path: alloc::string::String::from(path),
        };
        drop(vfs);
        return FD_TABLE.lock().alloc(entry);
    }

    match vfs_mut.read_file(path) {
        Ok(data) => {
            let entry = FdEntry::File { data, pos: 0 };
            drop(vfs);
            FD_TABLE.lock().alloc(entry)
        }
        Err(_) => -2, // ENOENT
    }
}

/// Read up to `buf.len()` bytes from `fd` into `buf`.
/// Returns the number of bytes read, 0 at EOF, or a negative error code.
pub fn read(fd: i64, buf: &mut [u8]) -> i64 {
    let mut table = FD_TABLE.lock();
    match table.get_mut(fd) {
        Some(FdEntry::File { data, pos }) => {
            if *pos >= data.len() {
                return 0; // EOF
            }
            let n = buf.len().min(data.len() - *pos);
            buf[..n].copy_from_slice(&data[*pos..*pos + n]);
            *pos += n;
            n as i64
        }
        Some(FdEntry::Directory { .. }) => -21, // EISDIR
        None => -9,                             // EBADF
    }
}

/// Close `fd`.  Returns 0 on success or a negative error code.
pub fn close(fd: i64) -> i64 {
    if FD_TABLE.lock().close(fd) { 0 } else { -9 } // EBADF
}

/// Fill `buf` with dirent64 records for the directory opened as `fd`.
/// Returns the number of bytes written, 0 when done, or a negative error code.
///
/// Each record has the Linux `dirent64` layout:
/// `ino(8) off(8) reclen(2) type(1) name(…NUL) pad-to-8`
pub fn getdents64(fd: i64, buf: &mut [u8]) -> i64 {
    let path = {
        let mut table = FD_TABLE.lock();
        match table.get_mut(fd) {
            Some(FdEntry::Directory { path }) => path.clone(),
            Some(FdEntry::File { .. }) => return -20, // ENOTDIR
            None => return -9,                        // EBADF
        }
    };

    let entries = {
        let mut vfs = crate::vfs::VFS.lock();
        match vfs.as_mut().and_then(|v| v.list_dir(&path).ok()) {
            Some(e) => e,
            None => return -2, // ENOENT
        }
    };

    let mut off = 0usize;
    for entry in &entries {
        let name_bytes = entry.name.as_bytes();
        let name_len = name_bytes.len();
        // reclen = 19 (fixed header) + name + NUL, rounded up to 8-byte boundary.
        let raw_len = 19 + name_len + 1;
        let reclen = (raw_len + 7) & !7;
        if off + reclen > buf.len() {
            break;
        }
        // inode (8 bytes) — fake
        buf[off..off + 8].fill(0);
        // offset (8 bytes) — fake
        buf[off + 8..off + 16].fill(0);
        // reclen (2 bytes, little-endian)
        let rl = reclen as u16;
        buf[off + 16] = rl as u8;
        buf[off + 17] = (rl >> 8) as u8;
        // type (1 byte): 4 = DT_DIR, 8 = DT_REG
        buf[off + 18] = match entry.node_type {
            crate::vfs::NodeType::Directory => 4,
            crate::vfs::NodeType::File => 8,
        };
        // name + NUL
        buf[off + 19..off + 19 + name_len].copy_from_slice(name_bytes);
        buf[off + 19 + name_len] = 0;
        // zero padding
        for b in buf[off + 19 + name_len + 1..off + reclen].iter_mut() {
            *b = 0;
        }
        off += reclen;
    }

    off as i64
}

/// Close all open file descriptors (called on process exit / exec cleanup).
pub fn close_all() {
    FD_TABLE.lock().close_all();
}
