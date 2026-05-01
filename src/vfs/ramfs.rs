//! RAM-backed in-memory filesystem.

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};

use super::{DirEntry, NodeType, VfsError, VfsResult};

enum Node {
    File(Vec<u8>),
    Directory(BTreeMap<String, Node>),
}

/// In-memory filesystem backed by a tree of `BTreeMap` nodes.
pub struct RamFs {
    root: Node,
    cwd: String,
}

impl RamFs {
    pub fn new() -> Self {
        RamFs {
            root: Node::Directory(BTreeMap::new()),
            cwd: String::from("/"),
        }
    }

    /// Returns the current working directory path.
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Changes the current working directory.
    pub fn cd(&mut self, path: &str) -> VfsResult<()> {
        let abs = self.resolve(path);
        match self.get_node(&abs)? {
            Node::Directory(_) => {
                self.cwd = abs;
                Ok(())
            }
            Node::File(_) => Err(VfsError::NotADirectory),
        }
    }

    /// Lists entries in `path` (or the cwd if `path` is empty).
    pub fn list_dir(&self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let abs = self.resolve(path);
        match self.get_node(&abs)? {
            Node::Directory(children) => Ok(children
                .iter()
                .map(|(name, node)| DirEntry {
                    name: name.clone(),
                    node_type: match node {
                        Node::File(_) => NodeType::File,
                        Node::Directory(_) => NodeType::Directory,
                    },
                })
                .collect()),
            Node::File(_) => Err(VfsError::NotADirectory),
        }
    }

    /// Creates a new directory at `path`.
    pub fn mkdir(&mut self, path: &str) -> VfsResult<()> {
        let abs = self.resolve(path);
        let (parent, name) = split_path(&abs).ok_or(VfsError::InvalidPath)?;
        match self.get_node_mut(&parent)? {
            Node::Directory(children) => {
                if children.contains_key(name) {
                    return Err(VfsError::AlreadyExists);
                }
                children.insert(name.to_string(), Node::Directory(BTreeMap::new()));
                Ok(())
            }
            Node::File(_) => Err(VfsError::NotADirectory),
        }
    }

    /// Writes (creates or overwrites) a file at `path`.
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> VfsResult<()> {
        let abs = self.resolve(path);
        let (parent, name) = split_path(&abs).ok_or(VfsError::InvalidPath)?;
        match self.get_node_mut(&parent)? {
            Node::Directory(children) => {
                children.insert(name.to_string(), Node::File(data.to_vec()));
                Ok(())
            }
            Node::File(_) => Err(VfsError::NotADirectory),
        }
    }

    /// Reads and returns the contents of a file at `path`.
    pub fn read_file(&self, path: &str) -> VfsResult<Vec<u8>> {
        let abs = self.resolve(path);
        match self.get_node(&abs)? {
            Node::File(data) => Ok(data.clone()),
            Node::Directory(_) => Err(VfsError::NotAFile),
        }
    }

    /// Removes a file or an empty directory at `path`.
    pub fn remove(&mut self, path: &str) -> VfsResult<()> {
        let abs = self.resolve(path);
        if abs == "/" {
            return Err(VfsError::InvalidPath);
        }
        let (parent, name) = split_path(&abs).ok_or(VfsError::InvalidPath)?;
        match self.get_node_mut(&parent)? {
            Node::Directory(children) => {
                if let Some(Node::Directory(sub)) = children.get(name) {
                    if !sub.is_empty() {
                        return Err(VfsError::DirectoryNotEmpty);
                    }
                }
                children.remove(name).ok_or(VfsError::NotFound)?;
                Ok(())
            }
            Node::File(_) => Err(VfsError::NotADirectory),
        }
    }

    /// Renames or moves `src` to `dst`.
    pub fn rename(&mut self, src: &str, dst: &str) -> VfsResult<()> {
        let abs_src = self.resolve(src);
        let abs_dst = self.resolve(dst);
        if abs_src == abs_dst {
            return Ok(());
        }
        if abs_src == "/" {
            return Err(VfsError::InvalidPath);
        }
        // Clone the source node, insert at dst, remove from src
        let node_clone = self.clone_node(&abs_src)?;
        let (dst_parent, dst_name) = split_path(&abs_dst).ok_or(VfsError::InvalidPath)?;
        let (src_parent, src_name) = split_path(&abs_src).ok_or(VfsError::InvalidPath)?;
        match self.get_node_mut(&dst_parent)? {
            Node::Directory(children) => {
                children.insert(dst_name.to_string(), node_clone);
            }
            Node::File(_) => return Err(VfsError::NotADirectory),
        }
        match self.get_node_mut(&src_parent)? {
            Node::Directory(children) => {
                children.remove(src_name);
            }
            Node::File(_) => return Err(VfsError::NotADirectory),
        }
        Ok(())
    }

    /// Copies a file from `src` to `dst`.
    pub fn copy(&mut self, src: &str, dst: &str) -> VfsResult<()> {
        let abs_src = self.resolve(src);
        let abs_dst = self.resolve(dst);
        let data = self.read_file(&abs_src)?;
        self.write_file(&abs_dst, &data)
    }

    // ---- internal helpers ----

    fn resolve(&self, path: &str) -> String {
        if path.starts_with('/') {
            normalize_path(path)
        } else if path.is_empty() || path == "." {
            self.cwd.clone()
        } else {
            let base = if self.cwd == "/" {
                String::from("/")
            } else {
                alloc::format!("{}/", self.cwd)
            };
            normalize_path(&alloc::format!("{}{}", base, path))
        }
    }

    fn get_node(&self, abs_path: &str) -> VfsResult<&Node> {
        if abs_path == "/" {
            return Ok(&self.root);
        }
        let mut current = &self.root;
        for part in abs_path.trim_start_matches('/').split('/') {
            if part.is_empty() {
                continue;
            }
            match current {
                Node::Directory(children) => {
                    current = children.get(part).ok_or(VfsError::NotFound)?;
                }
                Node::File(_) => return Err(VfsError::NotADirectory),
            }
        }
        Ok(current)
    }

    fn get_node_mut(&mut self, abs_path: &str) -> VfsResult<&mut Node> {
        if abs_path == "/" {
            return Ok(&mut self.root);
        }
        let parts: Vec<&str> = abs_path
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        let mut current = &mut self.root;
        for part in parts {
            match current {
                Node::Directory(children) => {
                    current = children.get_mut(part).ok_or(VfsError::NotFound)?;
                }
                Node::File(_) => return Err(VfsError::NotADirectory),
            }
        }
        Ok(current)
    }

    fn clone_node(&self, abs_path: &str) -> VfsResult<Node> {
        fn clone_inner(n: &Node) -> Node {
            match n {
                Node::File(data) => Node::File(data.clone()),
                Node::Directory(children) => Node::Directory(
                    children
                        .iter()
                        .map(|(k, v)| (k.clone(), clone_inner(v)))
                        .collect(),
                ),
            }
        }
        Ok(clone_inner(self.get_node(abs_path)?))
    }
}

/// Normalize an absolute path: resolve `.` and `..`, collapse double slashes.
fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        String::from("/")
    } else {
        alloc::format!("/{}", parts.join("/"))
    }
}

/// Split an absolute path into `(parent_dir, file_name)`.
/// Returns `None` for the root `/`.
fn split_path(abs_path: &str) -> Option<(String, &str)> {
    if abs_path == "/" {
        return None;
    }
    let trimmed = abs_path.trim_end_matches('/');
    let pos = trimmed.rfind('/')?;
    let parent = if pos == 0 {
        String::from("/")
    } else {
        trimmed[..pos].to_string()
    };
    let name = &trimmed[pos + 1..];
    Some((parent, name))
}
