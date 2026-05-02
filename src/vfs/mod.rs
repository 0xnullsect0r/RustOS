//! Virtual File System (VFS) abstraction.
//!
//! The VFS layer provides:
//! - An in-memory `RamFs` for the default `/` filesystem
//! - A `Filesystem` trait that external filesystems (e.g. FAT32 on USB) implement
//! - A global `MOUNTS` table routing paths to the correct filesystem

pub mod ramfs;
pub use ramfs::RamFs;

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use spin::Mutex;

// ---------------------------------------------------------------------------
// Error and result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VfsError {
    NotFound,
    AlreadyExists,
    NotADirectory,
    NotAFile,
    DirectoryNotEmpty,
    InvalidPath,
    IoError,
    ReadOnly,
}

impl core::fmt::Display for VfsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VfsError::NotFound => write!(f, "not found"),
            VfsError::AlreadyExists => write!(f, "already exists"),
            VfsError::NotADirectory => write!(f, "not a directory"),
            VfsError::NotAFile => write!(f, "not a file"),
            VfsError::DirectoryNotEmpty => write!(f, "directory not empty"),
            VfsError::InvalidPath => write!(f, "invalid path"),
            VfsError::IoError => write!(f, "I/O error"),
            VfsError::ReadOnly => write!(f, "read-only filesystem"),
        }
    }
}

pub type VfsResult<T> = Result<T, VfsError>;

#[derive(Debug, Clone)]
pub enum NodeType {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub node_type: NodeType,
}

// ---------------------------------------------------------------------------
// Filesystem trait
// ---------------------------------------------------------------------------

/// Any filesystem that can be mounted into the VFS must implement this trait.
pub trait Filesystem: Send {
    fn list_dir(&mut self, path: &str) -> VfsResult<Vec<DirEntry>>;
    fn read_file(&mut self, path: &str) -> VfsResult<Vec<u8>>;
    fn write_file(&mut self, path: &str, data: &[u8]) -> VfsResult<()>;
    fn mkdir(&mut self, path: &str) -> VfsResult<()>;
    fn remove(&mut self, path: &str) -> VfsResult<()>;
    fn rename(&mut self, src: &str, dst: &str) -> VfsResult<()>;
    fn copy(&mut self, src: &str, dst: &str) -> VfsResult<()>;
    fn is_dir(&mut self, path: &str) -> bool;
    fn exists(&mut self, path: &str) -> bool;
}

// ---------------------------------------------------------------------------
// RamFs adapter (implements Filesystem)
// ---------------------------------------------------------------------------

impl Filesystem for RamFs {
    fn list_dir(&mut self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let s: &Self = self; // coerce to &Self so inherent (&self) methods are chosen
        s.list_dir(path)
    }
    fn read_file(&mut self, path: &str) -> VfsResult<Vec<u8>> {
        let s: &Self = self;
        s.read_file(path)
    }
    fn write_file(&mut self, path: &str, data: &[u8]) -> VfsResult<()> {
        self.write_file(path, data)
    }
    fn mkdir(&mut self, path: &str) -> VfsResult<()> {
        self.mkdir(path)
    }
    fn remove(&mut self, path: &str) -> VfsResult<()> {
        self.remove(path)
    }
    fn rename(&mut self, src: &str, dst: &str) -> VfsResult<()> {
        self.rename(src, dst)
    }
    fn copy(&mut self, src: &str, dst: &str) -> VfsResult<()> {
        self.copy(src, dst)
    }
    fn is_dir(&mut self, path: &str) -> bool {
        let s: &Self = self;
        s.is_dir(path)
    }
    fn exists(&mut self, path: &str) -> bool {
        let s: &Self = self;
        s.exists(path)
    }
}

// ---------------------------------------------------------------------------
// FAT32 adapter
// ---------------------------------------------------------------------------

use crate::fs::fat32::Fat32Fs;

pub struct Fat32Mount(pub Fat32Fs);

impl Filesystem for Fat32Mount {
    fn list_dir(&mut self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let entries = self.0.list(path).ok_or(VfsError::NotFound)?;
        Ok(entries
            .into_iter()
            .map(|e| DirEntry {
                name: e.name,
                node_type: if e.is_dir {
                    NodeType::Directory
                } else {
                    NodeType::File
                },
            })
            .collect())
    }
    fn read_file(&mut self, path: &str) -> VfsResult<Vec<u8>> {
        self.0.read_file(path).ok_or(VfsError::IoError)
    }
    fn write_file(&mut self, _: &str, _: &[u8]) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
    fn mkdir(&mut self, _: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
    fn remove(&mut self, _: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
    fn rename(&mut self, _: &str, _: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
    fn copy(&mut self, _: &str, _: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }
    fn is_dir(&mut self, path: &str) -> bool {
        self.0.list(path).is_some()
    }
    fn exists(&mut self, path: &str) -> bool {
        self.0.lookup(path).is_some()
    }
}

// ---------------------------------------------------------------------------
// Global mount table
// ---------------------------------------------------------------------------

/// A mount point maps a path prefix to a filesystem.
struct MountPoint {
    prefix: String,
    fs: Box<dyn Filesystem>,
}

/// Global VFS state: the root RamFs + any additional mount points.
pub struct Vfs {
    root: RamFs,
    mounts: Vec<MountPoint>,
}

impl Vfs {
    pub fn new() -> Self {
        Vfs {
            root: RamFs::new(),
            mounts: Vec::new(),
        }
    }
}

impl Default for Vfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Vfs {
    /// Mount `fs` at `mount_path` (e.g. "/usb").
    pub fn mount(&mut self, mount_path: &str, fs: Box<dyn Filesystem>) {
        // Ensure the mount point directory exists in root
        let _ = self.root.mkdir(mount_path);
        self.mounts.push(MountPoint {
            prefix: normalize(mount_path),
            fs,
        });
    }

    fn find_mount(&self, norm_path: &str) -> Option<usize> {
        let mut best_len = 0usize;
        let mut best_idx: Option<usize> = None;
        for (i, mp) in self.mounts.iter().enumerate() {
            let prefix = &mp.prefix;
            if (norm_path == prefix.as_str()
                || norm_path.starts_with(&alloc::format!("{}/", prefix)))
                && prefix.len() > best_len
            {
                best_len = prefix.len();
                best_idx = Some(i);
            }
        }
        best_idx
    }

    /// Route a path: if it falls under a mount point prefix, use that filesystem;
    /// otherwise use the root RamFs.
    fn route(&mut self, path: &str) -> (&mut dyn Filesystem, String) {
        let norm = normalize(path);
        // Find the most-specific mount (longest prefix match)
        let mut best_len = 0usize;
        let mut best_idx: Option<usize> = None;
        for (i, mp) in self.mounts.iter().enumerate() {
            let prefix = &mp.prefix;
            if (norm == *prefix || norm.starts_with(&alloc::format!("{}/", prefix)))
                && prefix.len() > best_len
            {
                best_len = prefix.len();
                best_idx = Some(i);
            }
        }
        if let Some(idx) = best_idx {
            let prefix = self.mounts[idx].prefix.clone();
            let rel = if norm == prefix {
                String::from("/")
            } else {
                norm[prefix.len()..].to_string()
            };
            (&mut *self.mounts[idx].fs, rel)
        } else {
            let norm2 = norm.clone();
            (&mut self.root, norm2)
        }
    }

    pub fn list_dir(&mut self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let (fs, rel) = self.route(path);
        fs.list_dir(&rel)
    }

    pub fn read_file(&mut self, path: &str) -> VfsResult<Vec<u8>> {
        let (fs, rel) = self.route(path);
        fs.read_file(&rel)
    }

    pub fn write_file(&mut self, path: &str, data: &[u8]) -> VfsResult<()> {
        let (fs, rel) = self.route(path);
        fs.write_file(&rel, data)
    }

    pub fn mkdir(&mut self, path: &str) -> VfsResult<()> {
        let (fs, rel) = self.route(path);
        fs.mkdir(&rel)
    }

    pub fn remove(&mut self, path: &str) -> VfsResult<()> {
        let (fs, rel) = self.route(path);
        fs.remove(&rel)
    }

    pub fn rename(&mut self, src: &str, dst: &str) -> VfsResult<()> {
        // Determine which mount handles each path
        let norm_src = normalize(src);
        let norm_dst = normalize(dst);

        let src_idx = self.find_mount(&norm_src);
        let dst_idx = self.find_mount(&norm_dst);

        if src_idx != dst_idx {
            // Cross-filesystem rename: read + write + delete
            let data = {
                let (fs, rel) = self.route(&norm_src);
                fs.read_file(&rel)?
            };
            {
                let (fs, rel) = self.route(&norm_dst);
                fs.write_file(&rel, &data)?;
            }
            let (fs, rel) = self.route(&norm_src);
            fs.remove(&rel)
        } else {
            // Same filesystem — use native rename
            let (rel_src, rel_dst) = if let Some(idx) = src_idx {
                let prefix = &self.mounts[idx].prefix;
                let rs = if norm_src == *prefix {
                    String::from("/")
                } else {
                    norm_src[prefix.len()..].to_string()
                };
                let rd = if norm_dst == *prefix {
                    String::from("/")
                } else {
                    norm_dst[prefix.len()..].to_string()
                };
                (rs, rd)
            } else {
                (norm_src, norm_dst)
            };
            if let Some(idx) = src_idx {
                self.mounts[idx].fs.rename(&rel_src, &rel_dst)
            } else {
                self.root.rename(&rel_src, &rel_dst)
            }
        }
    }

    pub fn copy(&mut self, src: &str, dst: &str) -> VfsResult<()> {
        let norm_src = normalize(src);
        let norm_dst = normalize(dst);
        // Read from source
        let data = {
            let (fs, rel) = self.route(&norm_src);
            fs.read_file(&rel)?
        };
        // Write to destination
        let (fs, rel) = self.route(&norm_dst);
        fs.write_file(&rel, &data)
    }

    pub fn is_dir(&mut self, path: &str) -> bool {
        let (fs, rel) = self.route(path);
        fs.is_dir(&rel)
    }

    pub fn exists(&mut self, path: &str) -> bool {
        let (fs, rel) = self.route(path);
        fs.exists(&rel)
    }

    pub fn list_mounts(&self) -> Vec<String> {
        self.mounts.iter().map(|m| m.prefix.clone()).collect()
    }
}

fn normalize(path: &str) -> String {
    crate::vfs::ramfs::RamFs::pub_normalize(path)
}

// ---------------------------------------------------------------------------
// Global VFS singleton
// ---------------------------------------------------------------------------

pub static VFS: Mutex<Option<Vfs>> = Mutex::new(None);

pub fn init() {
    *VFS.lock() = Some(Vfs::new());
}
