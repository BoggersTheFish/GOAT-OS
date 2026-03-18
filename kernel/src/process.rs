//! Process abstraction: owns a Strongest Node plus execution resources.
//!
//! Phase 2.2: create a proper Process struct that contains its own Node
//! (activation, tension, neighbors) and its address space, stacks, and state.

use crate::scheduler::{NodeState, Vma, MAX_NEIGHBORS, MAX_VMAS, NEIGHBOR_NONE, PARENT_NONE};

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

pub struct Process {
    pub id: u32,
    pub node: Node,
    pub state: NodeState,

    // Address space
    pub cr3: u64,
    pub vmas: [Vma; MAX_VMAS],
    pub vma_count: usize,

    // Context
    pub saved_rsp: u64,
    pub saved_rip: u64,

    // Lifecycle
    pub parent: usize,
    pub exit_status: u8,
}

impl Process {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            node: Node::new(),
            state: NodeState::Ready,
            cr3: 0,
            vmas: [Vma::empty(); MAX_VMAS],
            vma_count: 0,
            saved_rsp: 0,
            saved_rip: 0,
            parent: PARENT_NONE,
            exit_status: 0,
        }
    }
}

