//! Virtual File System (VFS) abstraction.
//! Currently backed by an in-memory RAM filesystem (`RamFs`).

pub mod ramfs;
pub use ramfs::RamFs;

use alloc::string::String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VfsError {
    NotFound,
    AlreadyExists,
    NotADirectory,
    NotAFile,
    DirectoryNotEmpty,
    InvalidPath,
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
