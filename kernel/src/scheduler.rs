//! Strongest Node scheduler — activation/tension graph drives process selection.
//! Tension resolution remains the core mechanism; never round-robin.

use alloc::vec::Vec;

pub const MAX_NODES: usize = 32;
pub const MAX_NEIGHBORS: usize = 4;
pub const NEIGHBOR_NONE: u8 = 0xFF;
pub const STACK_SIZE: usize = 4096;
pub const CWD_MAX: usize = 64;
pub const PARENT_NONE: usize = 0xFF;

/// Virtual memory area (user space) for demand paging decisions.
#[derive(Clone, Copy)]
pub struct Vma {
    pub start: u64,
    pub end: u64,
    pub writable: bool,
    pub executable: bool,
}

impl Vma {
    pub const fn empty() -> Self {
        Self {
            start: 0,
            end: 0,
            writable: false,
            executable: false,
        }
    }

    pub fn contains(&self, addr: u64) -> bool {
        self.start != 0 && addr >= self.start && addr < self.end
    }
}

pub const MAX_VMAS: usize = 8;

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum NodeState {
    Ready = 0,
    Running = 1,
    Exited = 2,
    Waiting = 3,
}

pub struct ProcessNode {
    pub id: u32,
    pub activation: u32,
    pub tension: u32,
    pub state: NodeState,
    pub neighbors: [u8; MAX_NEIGHBORS],
    pub stack: [u8; STACK_SIZE],
    pub saved_rip: u64,
    pub saved_rsp: u64,
    pub cr3: u64,
    pub parent: usize,
    pub exit_status: u8,
    pub cwd: [u8; CWD_MAX],
    pub vmas: [Vma; MAX_VMAS],
    pub vma_count: usize,
}

impl ProcessNode {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            activation: 0,
            tension: 0,
            state: NodeState::Ready,
            neighbors: [NEIGHBOR_NONE; MAX_NEIGHBORS],
            stack: [0u8; STACK_SIZE],
            saved_rip: 0,
            saved_rsp: 0,
            cr3: 0,
            parent: PARENT_NONE,
            exit_status: 0,
            cwd: [0; CWD_MAX],
            vmas: [Vma::empty(); MAX_VMAS],
            vma_count: 0,
        }
    }

    pub fn effective_strength(&self) -> i32 {
        self.activation as i32 - self.tension as i32
    }
}

pub struct ProcessGraph {
    pub nodes: Vec<ProcessNode>,
}

impl ProcessGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
        }
    }

    pub fn count(&self) -> usize {
        self.nodes.len()
    }

    pub fn add_node(&mut self, id: u32, activation: u32, tension: u32, neighbors: &[u8], parent: usize) -> bool {
        if self.nodes.len() >= MAX_NODES {
            return false;
        }
        let mut cwd = [0u8; CWD_MAX];
        cwd[0] = b'/';
        cwd[1] = 0;
        self.nodes.push(ProcessNode {
            id,
            activation,
            tension,
            state: NodeState::Ready,
            neighbors: {
                let mut n = [NEIGHBOR_NONE; MAX_NEIGHBORS];
                for (j, &idx) in neighbors.iter().take(MAX_NEIGHBORS).enumerate() {
                    n[j] = idx;
                }
                n
            },
            stack: [0u8; STACK_SIZE],
            saved_rip: 0,
            saved_rsp: 0,
            cr3: 0,
            parent,
            exit_status: 0,
            cwd,
            vmas: [Vma::empty(); MAX_VMAS],
            vma_count: 0,
        });
        true
    }

    pub fn decay_all(&mut self, except: usize) {
        for i in 0..self.nodes.len() {
            if i != except {
                self.nodes[i].activation = self.nodes[i].activation.saturating_sub(2);
            }
        }
    }

    pub fn spread_from(&mut self, from: usize) {
        const SPREAD: u32 = 10;
        let neighbors: [u8; MAX_NEIGHBORS] = self.nodes[from].neighbors;
        for &idx in &neighbors {
            if idx == NEIGHBOR_NONE {
                break;
            }
            let i = idx as usize;
            if i < self.nodes.len() && i != from {
                self.nodes[i].activation = self.nodes[i].activation.saturating_add(SPREAD);
                if self.nodes[i].activation > 200 {
                    self.nodes[i].activation = 200;
                }
            }
        }
    }

    pub fn select_strongest(&self) -> Option<usize> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut best = None;
        let mut best_s = i32::MIN;
        for i in 0..self.nodes.len() {
            if matches!(self.nodes[i].state, NodeState::Exited | NodeState::Waiting) {
                continue;
            }
            let s = self.nodes[i].effective_strength();
            if s > best_s {
                best_s = s;
                best = Some(i);
            }
        }
        best
    }

    pub fn try_add_node(&mut self, id: u32, activation: u32, tension: u32, neighbors: &[u8], parent: usize) -> Option<usize> {
        if self.nodes.len() >= MAX_NODES {
            return None;
        }
        let i = self.nodes.len();
        self.add_node(id, activation, tension, neighbors, parent);
        Some(i)
    }

    /// Remove Exited nodes when count exceeds threshold. Frees slots for new nodes.
    /// Note: Currently a no-op; removing nodes would invalidate indices used elsewhere.
    /// Future: use stable IDs or compact-and-remap.
    #[allow(dead_code)]
    pub fn prune_dead_nodes(&mut self) {
        const _THRESHOLD: usize = 24;
        // self.nodes.retain(|n| !matches!(n.state, NodeState::Exited));
    }

    /// Stub for future: merge low-tension pairs when graph is crowded.
    #[allow(dead_code)]
    pub fn try_merge_low_tension_pairs(&mut self) {
        // TODO: when two nodes have very low tension and similar behavior, consider merging
    }
}
