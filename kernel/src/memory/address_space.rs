//! AddressSpace + VMA-as-node (Phase 1).
//!
//! AddressSpace owns CR3 and the set of VMAs that define valid user mappings.
//! Each VMA embeds a canonical Node so it participates in activation/tension.

extern crate alloc;

use alloc::vec::Vec;

use crate::memory::layout::{USER_LOWER_END, USER_LOWER_START, USER_NULL_GUARD_END};
use crate::scheduler::Node;

pub struct Vma {
    pub node: Node, // VMA participates in Strongest Node graph
    pub start: u64,
    pub size: u64,
    pub write: bool,
    pub exec: bool,
}

impl Vma {
    pub fn new(start: u64, size: u64, write: bool, exec: bool) -> Self {
        Self {
            node: Node::new(),
            start,
            size,
            write,
            exec,
        }
    }

    pub fn end(&self) -> u64 {
        self.start + self.size
    }

    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.start && addr < self.end()
    }

    pub fn is_user_sane(&self) -> bool {
        // basic sanity: stays in canonical user range and not in null guard
        self.start >= USER_LOWER_START
            && self.end() <= USER_LOWER_END
            && self.start >= USER_NULL_GUARD_END
            && self.size > 0
    }
}

pub struct AddressSpace {
    pub cr3: u64,
    pub vmas: Vec<Vma>,
}

impl AddressSpace {
    pub fn new(cr3: u64) -> Self {
        Self { cr3, vmas: Vec::new() }
    }

    pub fn covers(&self, addr: u64) -> Option<&Vma> {
        self.vmas.iter().find(|vma| vma.contains(addr))
    }

    pub fn covers_mut(&mut self, addr: u64) -> Option<&mut Vma> {
        self.vmas.iter_mut().find(|vma| vma.contains(addr))
    }
}

