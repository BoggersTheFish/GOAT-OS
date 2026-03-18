//! Per-process file descriptor table (Phase 3.2).

extern crate alloc;

use alloc::string::String;

#[derive(Clone, Copy)]
pub struct OpenFlags {
    pub write: bool,
}

#[derive(Clone)]
pub struct FdEntry {
    pub path: String,
    pub offset: usize,
    pub flags: OpenFlags,
}

pub struct FdTable {
    entries: [Option<FdEntry>; 32],
}

impl FdTable {
    pub const fn new() -> Self {
        const NONE: Option<FdEntry> = None;
        Self { entries: [NONE; 32] }
    }

    pub fn open(&mut self, path: &str, flags: OpenFlags) -> Option<u32> {
        for i in 0..self.entries.len() {
            if self.entries[i].is_none() {
                self.entries[i] = Some(FdEntry {
                    path: path.into(),
                    offset: 0,
                    flags,
                });
                return Some(i as u32);
            }
        }
        None
    }

    pub fn close(&mut self, fd: u32) -> bool {
        let i = fd as usize;
        if i >= self.entries.len() {
            return false;
        }
        self.entries[i] = None;
        true
    }
}

