//! Strongest Node scheduler — activation/tension graph drives process selection.
//! Tension resolution remains the core mechanism; never round-robin.

use alloc::vec::Vec;

use crate::process::Process;

pub const MAX_NODES: usize = 32;
pub const MAX_NEIGHBORS: usize = 4;
pub const NEIGHBOR_NONE: u8 = 0xFF;
pub const STACK_SIZE: usize = 4096;
pub const CWD_MAX: usize = 64;
pub const PARENT_NONE: usize = 0xFF;

/// Canonical Strongest Node for processes (and secondary nodes).
///
/// Kept in `scheduler` so all layers share one definition.
#[derive(Clone)]
pub struct Node {
    pub activation: u32,
    pub tension: u32,
    pub neighbors: [u8; MAX_NEIGHBORS],
}

impl Node {
    pub const fn new() -> Self {
        Self {
            activation: 0,
            tension: 0,
            neighbors: [NEIGHBOR_NONE; MAX_NEIGHBORS],
        }
    }
}

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum NodeState {
    Ready = 0,
    Running = 1,
    Exited = 2,
    Waiting = 3,
}

pub struct ProcessGraph {
    pub procs: Vec<Process>,
}

static mut GRAPH: Option<ProcessGraph> = None;
static mut CURRENT_NODE_IDX: usize = 0xFF;

pub unsafe fn init_global_graph(graph: ProcessGraph) {
    GRAPH = Some(graph);
}

pub fn graph_mut() -> &'static mut ProcessGraph {
    unsafe { GRAPH.as_mut().unwrap() }
}

pub fn set_current_idx(idx: Option<usize>) {
    unsafe {
        CURRENT_NODE_IDX = idx.unwrap_or(0xFF);
    }
}

pub fn current_idx() -> Option<usize> {
    unsafe {
        if CURRENT_NODE_IDX == 0xFF {
            None
        } else {
            Some(CURRENT_NODE_IDX)
        }
    }
}

pub fn current_process_mut() -> Option<&'static mut Process> {
    unsafe {
        let idx = current_idx()?;
        let g = GRAPH.as_mut()?;
        if idx >= g.procs.len() {
            return None;
        }
        Some(&mut g.procs[idx])
    }
}

pub fn prune_process(idx: usize, exit_status: u8) {
    unsafe {
        if let Some(g) = GRAPH.as_mut() {
            if idx < g.procs.len() {
                g.procs[idx].node.activation = 0;
                g.procs[idx].node.tension = u32::MAX;
                g.procs[idx].state = NodeState::Exited;
                g.procs[idx].exit_status = exit_status;
            }
        }
        if CURRENT_NODE_IDX == idx {
            CURRENT_NODE_IDX = 0xFF;
        }
    }
}

impl ProcessGraph {
    pub fn new() -> Self {
        Self {
            procs: Vec::new(),
        }
    }

    pub fn count(&self) -> usize {
        self.procs.len()
    }

    pub fn add_node(&mut self, id: u32, activation: u32, tension: u32, neighbors: &[u8], parent: usize) -> bool {
        if self.procs.len() >= MAX_NODES {
            return false;
        }
        let mut p = Process::new(id, 0);
        p.node.activation = activation;
        p.node.tension = tension;
        p.parent = parent;
        for (j, &idx) in neighbors.iter().take(MAX_NEIGHBORS).enumerate() {
            p.node.neighbors[j] = idx;
        }
        self.procs.push(p);
        true
    }

    pub fn decay_all(&mut self, except: usize) {
        for i in 0..self.procs.len() {
            if i != except {
                self.procs[i].node.activation = self.procs[i].node.activation.saturating_sub(2);
            }
        }
    }

    pub fn spread_from(&mut self, from: usize) {
        const SPREAD: u32 = 10;
        let neighbors: [u8; MAX_NEIGHBORS] = self.procs[from].node.neighbors;
        for &idx in &neighbors {
            if idx == NEIGHBOR_NONE {
                break;
            }
            let i = idx as usize;
            if i < self.procs.len() && i != from {
                self.procs[i].node.activation = self.procs[i].node.activation.saturating_add(SPREAD);
                if self.procs[i].node.activation > 200 {
                    self.procs[i].node.activation = 200;
                }
            }
        }
    }

    pub fn select_strongest(&self) -> Option<usize> {
        if self.procs.is_empty() {
            return None;
        }
        let mut best = None;
        let mut best_s = i32::MIN;
        for i in 0..self.procs.len() {
            if matches!(self.procs[i].state, NodeState::Exited | NodeState::Waiting) {
                continue;
            }
            let s = self.procs[i].node.activation as i32 - self.procs[i].node.tension as i32;
            if s > best_s {
                best_s = s;
                best = Some(i);
            }
        }
        best
    }

    pub fn try_add_node(&mut self, id: u32, activation: u32, tension: u32, neighbors: &[u8], parent: usize) -> Option<usize> {
        if self.procs.len() >= MAX_NODES {
            return None;
        }
        let i = self.procs.len();
        self.add_node(id, activation, tension, neighbors, parent);
        Some(i)
    }

    /// Remove Exited nodes when count exceeds threshold. Frees slots for new nodes.
    /// Note: Currently a no-op; removing nodes would invalidate indices used elsewhere.
    /// Future: use stable IDs or compact-and-remap.
    #[allow(dead_code)]
    pub fn prune_dead_nodes(&mut self) {
        const _THRESHOLD: usize = 24;
        // self.procs.retain(|p| !matches!(p.state, NodeState::Exited));
    }

    /// Stub for future: merge low-tension pairs when graph is crowded.
    #[allow(dead_code)]
    pub fn try_merge_low_tension_pairs(&mut self) {
        // TODO: when two nodes have very low tension and similar behavior, consider merging
    }
}
