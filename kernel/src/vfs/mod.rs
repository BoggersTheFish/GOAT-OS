//! Pluggable VFS layer (Phase 3.1).

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Copy)]
pub struct OpenFlags {
    pub write: bool,
}

pub type FileHandle = u32;

#[derive(Debug)]
pub enum VfsError {
    NotFound,
    NotDir,
    NoSpace,
    Unsupported,
}

pub trait Vfs {
    fn read_dir(&self, path: &str) -> Result<Vec<String>, VfsError>;
    fn read_file(&self, path: &str) -> Result<Vec<u8>, VfsError>;
    fn write_file(&self, path: &str, data: &[u8]) -> Result<(), VfsError>;
}

pub struct RamFs;

impl Vfs for RamFs {
    fn read_dir(&self, path: &str) -> Result<Vec<String>, VfsError> {
        Ok(crate::fs::list_dir(path))
    }

    fn read_file(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        crate::fs::read_file(path).ok_or(VfsError::NotFound)
    }

    fn write_file(&self, path: &str, data: &[u8]) -> Result<(), VfsError> {
        if crate::fs::write_file(path, data) {
            Ok(())
        } else {
            Err(VfsError::NoSpace)
        }
    }
}

/// Stub Disk FS (Phase 3.1): placeholder for FAT/ext2 later.
pub struct DiskFs;

impl Vfs for DiskFs {
    fn read_dir(&self, _path: &str) -> Result<Vec<String>, VfsError> {
        Err(VfsError::Unsupported)
    }
    fn read_file(&self, _path: &str) -> Result<Vec<u8>, VfsError> {
        Err(VfsError::Unsupported)
    }
    fn write_file(&self, _path: &str, _data: &[u8]) -> Result<(), VfsError> {
        Err(VfsError::Unsupported)
    }
}

pub struct MountPoint {
    pub path: String,
    pub fs: &'static dyn Vfs,
}

pub struct VfsLayer {
    pub mounts: Vec<MountPoint>,
    pub default: &'static dyn Vfs,
}

impl VfsLayer {
    pub fn new(default: &'static dyn Vfs) -> Self {
        Self {
            mounts: Vec::new(),
            default,
        }
    }

    pub fn mount(&mut self, path: &str, fs: &'static dyn Vfs) {
        self.mounts.push(MountPoint {
            path: path.into(),
            fs,
        });
    }

    fn resolve(&self, path: &str) -> &'static dyn Vfs {
        // Longest-prefix match (simple)
        let mut best: Option<&MountPoint> = None;
        for m in &self.mounts {
            if path.starts_with(&m.path) {
                if best.as_ref().map(|b| b.path.len()).unwrap_or(0) < m.path.len() {
                    best = Some(m);
                }
            }
        }
        best.map(|m| m.fs).unwrap_or(self.default)
    }

    pub fn read_dir(&self, path: &str) -> Result<Vec<String>, VfsError> {
        self.resolve(path).read_dir(path)
    }
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        self.resolve(path).read_file(path)
    }
    pub fn write_file(&self, path: &str, data: &[u8]) -> Result<(), VfsError> {
        self.resolve(path).write_file(path, data)
    }
}

