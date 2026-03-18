//! Canonical process model (Phase 2).
//!
//! A `Process` is a Strongest Node plus owned execution resources:
//! - `node`: activation/tension/neighbors (Strongest Node scheduler inputs)
//! - `aspace`: CR3 + VMAs (each VMA is a secondary node)
//! - `context`: saved CPU register state (matches trap/save ABI)
//! - `stack`: per-process kernel stack backing (kept inline for P2.1 to preserve behavior)

use crate::memory::address_space::AddressSpace;
use crate::scheduler::{Node, NodeState, CWD_MAX, PARENT_NONE};

/// Saved register state for a process.
///
/// IMPORTANT: Field order is chosen to match the *in-memory* layout of the
/// register save area produced by the current `syscall_stub` in `main.rs`:
///
/// - Assembly push order (first -> last): rax, rbx, rcx, rdx, rsi, rdi, rbp, r8, r9, r10, r11, r12, r13, r14, r15
/// - Because x86_64 `push` grows the stack downward, the pointer passed to `syscall_handler`
///   points at the **last pushed** register, `r15`.
/// - Therefore, this struct starts with `r15` and ends with `rax`.
///
/// The trailing `rip/cs/rflags/rsp/ss` are reserved for the eventual unified trap/iret frame
/// representation (P2.2/P2.3).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcessContext {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,

    // The CPU-pushed iretq frame is *below* these registers on the stack for int/traps.
    // We keep slots here for the eventual "fully materialized" context (P2.2/P2.3),
    // but P2.1 continues to treat `saved_rsp` as the authoritative stack frame pointer.
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl ProcessContext {
    pub const fn zero() -> Self {
        Self {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            r11: 0,
            r10: 0,
            r9: 0,
            r8: 0,
            rbp: 0,
            rdi: 0,
            rsi: 0,
            rdx: 0,
            rcx: 0,
            rbx: 0,
            rax: 0,
            rip: 0,
            cs: 0,
            rflags: 0,
            rsp: 0,
            ss: 0,
        }
    }
}

/// Typed view of the register-save area passed to `syscall_handler`.
///
/// This is the exact stack layout produced by `syscall_stub` *at the instant it calls*
/// into Rust:
///
/// ```text
/// push rax, rbx, rcx, rdx, rsi, rdi, rbp, r8, r9, r10, r11, r12, r13, r14, r15
/// mov  rdi, rsp   ; rdi = &TrapFrame (points at r15)
/// call syscall_handler
/// ```
///
/// So the Rust function receives a pointer to `r15` (the last pushed register).
#[repr(C)]
pub struct TrapFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
}

impl TrapFrame {
    #[inline]
    pub fn syscall_number(&self) -> u64 {
        self.rax
    }
}

impl ProcessContext {
    #[inline]
    pub fn from_trap_frame(tf: &TrapFrame) -> Self {
        Self {
            r15: tf.r15,
            r14: tf.r14,
            r13: tf.r13,
            r12: tf.r12,
            r11: tf.r11,
            r10: tf.r10,
            r9: tf.r9,
            r8: tf.r8,
            rbp: tf.rbp,
            rdi: tf.rdi,
            rsi: tf.rsi,
            rdx: tf.rdx,
            rcx: tf.rcx,
            rbx: tf.rbx,
            rax: tf.rax,
            // iret frame is not materialized here yet (P2.3)
            rip: 0,
            cs: 0,
            rflags: 0,
            rsp: 0,
            ss: 0,
        }
    }

    #[inline]
    pub fn write_to_trap_frame(&self, tf: &mut TrapFrame) {
        tf.r15 = self.r15;
        tf.r14 = self.r14;
        tf.r13 = self.r13;
        tf.r12 = self.r12;
        tf.r11 = self.r11;
        tf.r10 = self.r10;
        tf.r9 = self.r9;
        tf.r8 = self.r8;
        tf.rbp = self.rbp;
        tf.rdi = self.rdi;
        tf.rsi = self.rsi;
        tf.rdx = self.rdx;
        tf.rcx = self.rcx;
        tf.rbx = self.rbx;
        tf.rax = self.rax;
    }
}

pub struct Process {
    pub id: u32,
    pub node: Node,
    pub state: NodeState,
    pub aspace: AddressSpace,
    pub context: ProcessContext,
    pub kernel_stack_base: u64,
    pub kernel_stack_top: u64,
    pub parent: usize,
    pub exit_status: u8,
    pub cwd: [u8; CWD_MAX],
    /// Legacy compatibility for P2.1: pointer to the saved trap/syscall frame on `stack`.
    /// This will be replaced by fully restoring from `context` in P2.3.
    pub saved_rsp: u64,
    /// Legacy compatibility for P2.1.
    pub saved_rip: u64,
}

impl Process {
    pub fn new(id: u32, cr3: u64) -> Self {
        let mut cwd = [0u8; CWD_MAX];
        cwd[0] = b'/';
        Self {
            id,
            node: Node::new(),
            state: NodeState::Ready,
            aspace: AddressSpace::new(cr3),
            context: ProcessContext::zero(),
            kernel_stack_base: 0,
            kernel_stack_top: 0,
            parent: PARENT_NONE,
            exit_status: 0,
            cwd,
            saved_rsp: 0,
            saved_rip: 0,
        }
    }

    pub fn empty() -> Self {
        Self::new(0, 0)
    }
}

