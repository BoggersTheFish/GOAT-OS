//! TS-OS Kernel — Strongest Node Framework
//!
//! Kernel = core_engine (structural search, pattern discovery, constraint resolution)
//! Scheduler = pure Strongest Node weighted spreading activation
//! PIT timer interrupt + preemptive scheduler + bump heap

#![no_std]
#![no_main]
#![feature(alloc_error_handler, abi_x86_interrupt)]
#![allow(dead_code, static_mut_refs)]

extern crate alloc;

mod allocator;
mod disk;
mod drivers;
mod elf;
mod fs;
mod keyboard;
mod memory;
mod paging;
mod persist;
mod process;
mod scheduler;
mod shell;
mod vfs;
mod vga;
mod fd_table;

use scheduler::{NodeState, ProcessGraph, ProcessNode, CWD_MAX, MAX_NODES, PARENT_NONE, STACK_SIZE};
use scheduler::{Vma, MAX_VMAS};

use alloc::boxed::Box;
use alloc::string::ToString;
use core::alloc::Layout;
use core::arch::asm;
use core::mem::MaybeUninit;

use limine::request::{FramebufferRequest, HhdmRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker};
use limine::BaseRevision;
use x86_64::instructions::segmentation::{Segment, CS, SS};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::PrivilegeLevel;
use x86_64::VirtAddr;

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();
#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();
#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();
#[used]
#[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

const COM1: u16 = 0x3F8;
const LCR: u16 = COM1 + 3;
const LSR: u16 = COM1 + 5;
const DLL: u16 = COM1 + 0;
const DLM: u16 = COM1 + 1;
const FCR: u16 = COM1 + 2;

const PIT_CH0: u16 = 0x40;
const PIT_CMD: u16 = 0x43;
const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const IRQ_TIMER: u8 = 32;
const SYSCALL_VECTOR: u8 = 0x80;
const KERNEL_STACK_SIZE: usize = 4096;

static mut KERNEL_RSP: u64 = 0;
static mut CURRENT_NODE_IDX: usize = 0xFF;
static mut TICK_COUNT: u32 = 0;
static mut HHDM_OFFSET: u64 = 0;
static mut KERNEL_CR3: u64 = 0;

// Frame allocator bitmap storage (u64 words). 262_144 words = 2 MiB bitmap => 16 GiB coverage.
#[repr(align(4096))]
struct FrameBitmap([u64; 262_144]);
static mut FRAME_BITMAP: FrameBitmap = FrameBitmap([0; 262_144]);

core::arch::global_asm!(
    r#"
    .section .text._start
    .global _start
_start:
    call kmain
    hlt
    jmp _start

    .text
    .global syscall_stub
    .global double_fault_stub
    .global bootstrap_switch
    .global kernel_hlt_loop

double_fault_stub:
    mov rdi, rsp
    call double_fault_handler
    ud2

syscall_stub:
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov rdi, rsp
    call syscall_handler
    cmp rax, 0
    jz .no_switch_syscall
    mov rsp, rax
.no_switch_syscall:
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq

bootstrap_switch:
    push 0x10
    lea rax, [rsp + 40]
    push rax
    push 0x202
    push 0x08
    lea rax, [rip + kernel_hlt_loop]
    push rax
    mov rax, rsp
    mov [rsi], rax
    mov rsp, rdi
    iretq

kernel_hlt_loop:
    sti
.hlt_loop:
    hlt
    jmp .hlt_loop
"#
);

extern "C" {
    fn syscall_stub();
    fn double_fault_stub();
    fn bootstrap_switch(shell_rsp: u64, kernel_rsp_addr: *mut u64) -> !;
}

extern "x86-interrupt" fn timer_handler_x86(frame: InterruptStackFrame) {
    unsafe {
        outb(PIC1_CMD, 0x20);
        TICK_COUNT += 1;
        if TICK_COUNT % 3000 == 0 && TICK_COUNT > 0 {
            do_checkpoint();
        }
        let cur = if CURRENT_NODE_IDX != 0xFF {
            Some(CURRENT_NODE_IDX)
        } else {
            None
        };
        let frame_ptr = &frame as *const _ as *mut u8;
        if let Some(idx) = cur {
            graph().nodes[idx].saved_rsp = frame_ptr as u64;
            graph().nodes[idx].tension = graph().nodes[idx].tension.saturating_add(1);
        } else {
            KERNEL_RSP = frame_ptr as u64;
        }
        maybe_emerge_node();
        let current = graph().select_strongest();
        if let Some(c) = current {
            graph().decay_all(c);
            graph().spread_from(c);
        }
        let new_strongest = graph().select_strongest();
        if TICK_COUNT % 10 == 0 {
            if let Some(ns) = new_strongest {
                vga::update_status_bar(graph().count(), graph().nodes[ns].activation, graph().nodes[ns].tension);
            } else {
                vga::update_status_bar(graph().count(), 0, 0);
            }
        }
        if let Some(ns) = new_strongest {
            graph().nodes[ns].state = NodeState::Running;
            let do_switch = cur != Some(ns) || cur.is_none();
            if do_switch {
                CURRENT_NODE_IDX = ns;
                // Update IST[0] to the selected node's kernel IST stack.
                let tss = unsafe { &mut *TSS.as_mut_ptr() };
                let top = VirtAddr::new(unsafe { &KERNEL_IST_STACKS[ns] as *const _ as u64 } + 4096);
                tss.interrupt_stack_table[0] = top;
                paging::switch_cr3(graph().nodes[ns].cr3);
                let rsp = if graph().nodes[ns].saved_rsp != 0 {
                    graph().nodes[ns].saved_rsp
                } else {
                    graph().nodes[ns].stack.as_ptr().add(STACK_SIZE - 40) as u64
                };
                switch_to_rsp(rsp);
            }
        } else {
            paging::switch_cr3(KERNEL_CR3);
        }
    }
}

extern "x86-interrupt" fn page_fault_handler(_frame: InterruptStackFrame, _error: PageFaultErrorCode) {
    let addr = Cr2::read().as_u64();

    // Canonical lower half: bit47 sign-extended; reject non-canonical and upper half
    let canonical = ((addr >> 48) == 0) && ((addr >> 47) & 1 == 0);
    let in_user = addr >= 0x1000 && addr <= 0x0000_7FFF_FFFF_FFFFu64;

    if !canonical || !in_user {
        prune_current_on_segv(addr);
        return;
    }

    // VMA check: must be covered by some VMA for this process
    let idx = unsafe { CURRENT_NODE_IDX };
    if idx == 0xFF {
        prune_current_on_segv(addr);
        return;
    }

    let node = unsafe { &graph().nodes[idx] };
    let mut ok = false;
    for i in 0..node.vma_count.min(MAX_VMAS) {
        if node.vmas[i].contains(addr) {
            ok = true;
            break;
        }
    }
    if !ok {
        prune_current_on_segv(addr);
        return;
    }

    // Demand map one page (present, user, writable per VMA if found)
    let writable = node
        .vmas
        .iter()
        .take(node.vma_count.min(MAX_VMAS))
        .find(|v| v.contains(addr))
        .map(|v| v.writable)
        .unwrap_or(false);

    let page = addr & !0xFFFu64;
    // Allocate a physical frame via paging's allocator-backed table allocator and map it.
    // For now we reuse alloc_frame inside paging::map_page path by allocating a frame via that allocator.
    // We allocate a backing page using the global allocator (heap) then map its physical address.
    let layout = Layout::from_size_align(4096, 4096).unwrap();
    let ptr = unsafe { alloc::alloc::alloc(layout) };
    if ptr.is_null() {
        prune_current_on_segv(addr);
        return;
    }
    unsafe { core::ptr::write_bytes(ptr, 0, 4096) };
    let phys = (ptr as u64).saturating_sub(unsafe { HHDM_OFFSET });
    let _ = paging::map_page(unsafe { graph().nodes[idx].cr3 }, unsafe { HHDM_OFFSET }, page, phys, writable, true);
}

fn prune_current_on_segv(addr: u64) {
    console_write("Segmentation fault — node pruned due to invalid VA access\r\n");
    serial_write("SEGV @ ");
    serial_write_hex(addr);
    serial_write("\r\n");

    unsafe {
        if CURRENT_NODE_IDX != 0xFF {
            let idx = CURRENT_NODE_IDX;
            graph().nodes[idx].activation = 0;
            graph().nodes[idx].state = NodeState::Exited;
            graph().nodes[idx].exit_status = 139; // 128+SIGSEGV
            CURRENT_NODE_IDX = 0xFF;
        }
    }

    // Reschedule immediately by forcing a yield path
    let _ = do_schedule(None);
}

#[no_mangle]
pub unsafe extern "C" fn switch_to_rsp(rsp: u64) -> ! {
    asm!(
        "mov rsp, {}",
        "iretq",
        in(reg) rsp,
        options(noreturn)
    );
}

#[no_mangle]
pub unsafe extern "C" fn double_fault_handler(_frame: *mut u8) -> ! {
    serial_write("DF\r\n");
    hcf();
}

#[repr(align(16))]
struct KernelStack([u8; KERNEL_STACK_SIZE]);
static mut KERNEL_STACK: KernelStack = KernelStack([0; KERNEL_STACK_SIZE]);

#[repr(align(16))]
struct DoubleFaultStack([u8; 4096]);
static mut DOUBLE_FAULT_STACK: DoubleFaultStack = DoubleFaultStack([0; 4096]);

#[repr(align(16))]
struct TimerStack([u8; 4096]);
static mut TIMER_STACK: TimerStack = TimerStack([0; 4096]);

static mut TSS: MaybeUninit<TaskStateSegment> = MaybeUninit::uninit();

#[repr(align(16))]
struct KernelIstStack([u8; 4096]);
static mut KERNEL_IST_STACKS: [KernelIstStack; MAX_NODES] = [const { KernelIstStack([0; 4096]) }; MAX_NODES];


#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    serial_write("ALLOC ERR\r\n");
    hcf();
}

#[inline(always)]
unsafe fn outb(port: u16, byte: u8) {
    asm!("out dx, al", in("dx") port, in("al") byte, options(nostack, preserves_flags));
}

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let ret: u8;
    asm!("in al, dx", in("dx") port, out("al") ret, options(nostack, preserves_flags));
    ret
}

fn serial_read_byte() -> Option<u8> {
    unsafe {
        if (inb(LSR) & 1) == 0 {
            return None;
        }
        Some(inb(COM1))
    }
}

fn serial_has_byte() -> bool {
    unsafe { (inb(LSR) & 1) != 0 }
}

fn serial_init() {
    unsafe {
        outb(LCR, 0x80);
        outb(DLL, 0x01);
        outb(DLM, 0x00);
        outb(LCR, 0x03);
        outb(FCR, 0xC7);
    }
}

pub(crate) fn serial_write(s: &str) {
    for b in s.bytes() {
        unsafe { outb(COM1, b) };
    }
}

fn console_write(s: &str) {
    if vga::is_enabled() {
        vga::write_str(s);
    }
    serial_write(s);
}

fn serial_write_u32(n: u32) {
    let mut buf = [0u8; 12];
    let mut i = 0;
    let mut n = n;
    if n == 0 {
        serial_write("0");
        return;
    }
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        unsafe { outb(COM1, buf[i]) };
    }
}

fn serial_write_hex(n: u64) {
    const HEX: [u8; 16] = *b"0123456789ABCDEF";
    serial_write("0x");
    let mut first = true;
    for shift in (0..64).rev().step_by(4) {
        let nibble = ((n >> shift) & 0xF) as usize;
        if nibble != 0 || !first || shift == 0 {
            unsafe { outb(COM1, HEX[nibble]) };
            first = false;
        }
    }
    if first {
        unsafe { outb(COM1, b'0') };
    }
}


static mut GRAPH: Option<ProcessGraph> = None;

fn graph() -> &'static mut ProcessGraph {
    unsafe { GRAPH.as_mut().unwrap() }
}

fn small_delay() {
    for _ in 0..1000 {
        core::hint::spin_loop();
    }
}

fn do_sys_write(buf: &[u8]) {
    unsafe {
        asm!(
            "int 0x80",
            in("rax") SYS_WRITE,
            in("rdi") 1u64,
            in("rsi") buf.as_ptr(),
            in("rdx") buf.len(),
            options(nostack, preserves_flags)
        );
    }
}

fn do_sys_yield() {
    unsafe {
        asm!("int 0x80", in("rax") SYS_YIELD, options(nostack, preserves_flags));
    }
}

fn do_sys_read(fd: u64, buf: *mut u8, len: usize) -> usize {
    let ret: usize;
    unsafe {
        asm!(
            "int 0x80",
            in("rax") SYS_READ,
            in("rdi") fd,
            in("rsi") buf,
            in("rdx") len,
            lateout("rax") ret,
            options(nostack, preserves_flags)
        );
    }
    ret
}

fn do_sys_exit(status: u64) -> ! {
    unsafe {
        asm!(
            "int 0x80",
            in("rax") SYS_EXIT,
            in("rdi") status,
            options(nostack, preserves_flags)
        );
    }
    loop {
        core::hint::spin_loop();
    }
}

fn do_sys_spawn() -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "int 0x80",
            in("rax") SYS_SPAWN,
            lateout("rax") ret,
            options(nostack, preserves_flags)
        );
    }
    ret
}

unsafe fn pic_remap() {
    outb(PIC1_CMD, 0x11);
    outb(PIC2_CMD, 0x11);
    outb(PIC1_DATA, IRQ_TIMER);
    outb(PIC2_DATA, IRQ_TIMER + 8);
    outb(PIC1_DATA, 0x04);
    outb(PIC2_DATA, 0x02);
    outb(PIC1_DATA, 0x01);
    outb(PIC2_DATA, 0x01);
    // Unmask IRQ0 (timer); mask all others
    outb(PIC1_DATA, 0xFE);
    outb(PIC2_DATA, 0xFF);
}

unsafe fn pit_init() {
    let divisor = 11932u16;
    outb(PIT_CMD, 0x34);
    outb(PIT_CH0, (divisor & 0xFF) as u8);
    outb(PIT_CH0, (divisor >> 8) as u8);
}

#[no_mangle]
unsafe extern "C" fn yield_to_kernel() {
    let idx = CURRENT_NODE_IDX;
    if idx == 0xFF {
        return;
    }
    let node = &mut graph().nodes[idx];
    let rsp: u64;
    let rip: u64;
    asm!("mov {}, rsp", out(reg) rsp, options(nostack, preserves_flags));
    asm!("mov {}, [rsp]", out(reg) rip, options(nostack, preserves_flags));
    node.saved_rsp = rsp;
    node.saved_rip = rip;
    asm!("mov rsp, {}", in(reg) KERNEL_RSP, options(nostack, preserves_flags));
}

const SYS_WRITE: u64 = 1;
const SYS_YIELD: u64 = 2;
const SYS_SPAWN: u64 = 3;
const SYS_READ: u64 = 4;
const SYS_EXIT: u64 = 5;
const SYS_LS: u64 = 6;
const SYS_CAT: u64 = 7;
const SYS_PS: u64 = 8;
const SYS_TOUCH: u64 = 9;
const SYS_MKDIR: u64 = 10;
const SYS_WRITE_F: u64 = 11;
const SYS_SHUTDOWN: u64 = 12;
const SYS_CLEAR: u64 = 13;
const SYS_POLL_KEY: u64 = 14;
const SYS_RM: u64 = 15;
const SYS_GETPID: u64 = 16;
const SYS_CHDIR: u64 = 17;
const SYS_GETCWD: u64 = 18;
const SYS_WAIT: u64 = 19;
const SYS_KILL: u64 = 20;
const SYS_EXECVE: u64 = 21;
const SYS_FORK: u64 = 22;

fn do_checkpoint() {
    let mut graph_buf = [0u8; 64];
    graph_buf[0] = graph().count() as u8;
    let mut fs_buf = [0u8; 16384];
    let fs_len = fs::serialize_to(&mut fs_buf);
    persist::do_checkpoint(graph_buf.as_ptr(), 64, fs_buf.as_ptr(), fs_len);
}

fn maybe_emerge_node() {
    // prune_dead_nodes would invalidate indices; defer until we have stable IDs
    let mut max_tension = 0u32;
    for i in 0..graph().count() {
        if !matches!(graph().nodes[i].state, NodeState::Exited) {
            max_tension = max_tension.max(graph().nodes[i].tension);
        }
    }
    if max_tension > 30 && graph().count() < MAX_NODES {
        let next_id = graph().count() as u32;
        let parent = unsafe {
            if CURRENT_NODE_IDX != 0xFF {
                CURRENT_NODE_IDX
            } else {
                PARENT_NONE
            }
        };
        if let Some(idx) = graph().try_add_node(next_id, 50, 0, &[], parent) {
            init_node_stacks_for(&mut graph().nodes[idx]);
            graph().nodes[idx].state = NodeState::Ready;
        }
    }
}

fn validate_buf(ptr: *const u8, len: usize) -> bool {
    if ptr.is_null() {
        return false;
    }
    ptr as usize <= usize::MAX - len
}

fn do_schedule(cur: Option<usize>) -> u64 {
    maybe_emerge_node();
    let current = graph().select_strongest();
    if let Some(c) = current {
        graph().decay_all(c);
        graph().spread_from(c);
    }
    let new_strongest = graph().select_strongest();
    if let Some(ns) = new_strongest {
        graph().nodes[ns].state = NodeState::Running;
        let do_switch = cur != Some(ns) || cur.is_none();
        if do_switch {
            unsafe {
                CURRENT_NODE_IDX = ns;
                paging::switch_cr3(graph().nodes[ns].cr3);
            }
            if graph().nodes[ns].saved_rsp != 0 {
                return graph().nodes[ns].saved_rsp;
            }
            return unsafe { graph().nodes[ns].stack.as_ptr().add(STACK_SIZE - INIT_FRAME_SIZE) as u64 };
        }
    } else {
        unsafe { paging::switch_cr3(KERNEL_CR3) };
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn syscall_handler(frame_ptr: *mut u8) -> u64 {
    let frame = frame_ptr as *const u64;
    let syscall = *frame.add(0);
    match syscall {
        SYS_WRITE => {
            let _fd = *frame.add(5);
            let buf = *frame.add(4) as *const u8;
            let len = *frame.add(3) as usize;
            if !validate_buf(buf, len) {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            for i in 0..len {
                let b = *buf.add(i);
                vga::write_byte(b);
                outb(COM1, b);
            }
            *(frame_ptr as *mut u64) = len as u64;
            0
        }
        SYS_READ => {
            let fd = *frame.add(5);
            let buf = *frame.add(4) as *mut u8;
            let len = *frame.add(3) as usize;
            let mut n = 0usize;
            if fd == 0 && len > 0 && validate_buf(buf, len) {
                if let Some(c) = keyboard::read_byte().or_else(serial_read_byte) {
                    unsafe { *buf.add(0) = c };
                    n = 1;
                }
            }
            *(frame_ptr as *mut u64) = n as u64;
            0
        }
        SYS_YIELD => {
            if CURRENT_NODE_IDX != 0xFF {
                graph().nodes[CURRENT_NODE_IDX].saved_rsp = frame_ptr as u64;
            }
            let cur = if CURRENT_NODE_IDX != 0xFF {
                Some(CURRENT_NODE_IDX)
            } else {
                None
            };
            do_schedule(cur)
        }
        SYS_SPAWN => {
            let next_id = graph().count() as u32;
            let frame_mut = frame_ptr as *mut u64;
            let parent = if CURRENT_NODE_IDX != 0xFF {
                CURRENT_NODE_IDX
            } else {
                PARENT_NONE
            };
            if let Some(idx) = graph().try_add_node(next_id, 50, 0, &[], parent) {
                init_node_stacks_for(&mut graph().nodes[idx]);
                graph().nodes[idx].state = NodeState::Ready;
                *frame_mut.add(0) = idx as u64;
            } else {
                *frame_mut.add(0) = !0u64;
            }
            0
        }
        SYS_EXECVE => {
            // execve(path, path_len) - minimal: load file bytes from in-RAM FS and parse ELF
            if CURRENT_NODE_IDX == 0xFF {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let path_ptr = *frame.add(5) as *const u8;
            let path_len = *frame.add(4) as usize;
            if !validate_buf(path_ptr, path_len.min(64)) {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = unsafe { *path_ptr.add(i) };
            }
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");
            let bytes = fs::read_file(path).unwrap_or_default();
            if bytes.is_empty() {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let cr3 = graph().nodes[CURRENT_NODE_IDX].cr3;
            match elf::load_elf(&bytes, cr3, unsafe { HHDM_OFFSET }) {
                Ok(info) => {
                    // Set entry point by rewriting saved frame RIP slot
                    graph().nodes[CURRENT_NODE_IDX].saved_rip = info.entry;
                    *(frame_ptr as *mut u64) = 0;
                }
                Err(_) => {
                    *(frame_ptr as *mut u64) = !0u64;
                }
            }
            0
        }
        SYS_FORK => {
            // Minimal stub: not implemented yet (full address space clone later)
            *(frame_ptr as *mut u64) = !0u64;
            0
        }
        SYS_EXIT => {
            let status = *frame.add(5) as u8;
            if CURRENT_NODE_IDX != 0xFF {
                let idx = CURRENT_NODE_IDX;
                graph().nodes[idx].state = NodeState::Exited;
                graph().nodes[idx].exit_status = status;
                graph().nodes[idx].saved_rsp = frame_ptr as u64;
                let parent = graph().nodes[idx].parent;
                if parent != PARENT_NONE && parent < graph().count() {
                    if matches!(graph().nodes[parent].state, NodeState::Waiting) {
                        graph().nodes[parent].state = NodeState::Ready;
                        let parent_rsp = graph().nodes[parent].saved_rsp;
                        if parent_rsp != 0 {
                            unsafe {
                                *(parent_rsp as *mut u64) = (graph().nodes[idx].id as u64) << 8 | status as u64;
                            }
                        }
                    }
                }
            }
            let cur = if CURRENT_NODE_IDX != 0xFF {
                Some(CURRENT_NODE_IDX)
            } else {
                None
            };
            CURRENT_NODE_IDX = 0xFF;
            do_schedule(cur)
        }
        SYS_LS => {
            let path_ptr = *frame.add(5) as *const u8;
            let path_len = *frame.add(4) as usize;
            let out_ptr = *frame.add(3) as *mut u8;
            let out_len = *frame.add(2) as usize;
            if !validate_buf(path_ptr, path_len.min(64)) || !validate_buf(out_ptr, out_len) {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = unsafe { *path_ptr.add(i) };
            }
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("/");
            let entries = fs::list_dir(path);
            let mut n = 0usize;
            for e in &entries {
                let s = alloc::format!("{}\r\n", e);
                for b in s.bytes() {
                    if n < out_len {
                        unsafe { *out_ptr.add(n) = b };
                        n += 1;
                    }
                }
            }
            *(frame_ptr as *mut u64) = n as u64;
            0
        }
        SYS_CAT => {
            let path_ptr = *frame.add(5) as *const u8;
            let path_len = *frame.add(4) as usize;
            let out_ptr = *frame.add(3) as *mut u8;
            let out_len = *frame.add(2) as usize;
            if !validate_buf(path_ptr, path_len.min(64)) || !validate_buf(out_ptr, out_len) {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = unsafe { *path_ptr.add(i) };
            }
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");
            let content = fs::cat(path).unwrap_or_else(|| "".to_string());
            let mut n = 0usize;
            for b in content.bytes() {
                if n < out_len {
                    unsafe { *out_ptr.add(n) = b };
                    n += 1;
                }
            }
            *(frame_ptr as *mut u64) = n as u64;
            0
        }
        SYS_PS => {
            let out_ptr = *frame.add(5) as *mut u8;
            let out_len = *frame.add(4) as usize;
            if !validate_buf(out_ptr, out_len) {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let mut s = alloc::format!("pid  act  ten  state\r\n");
            for i in 0..graph().count() {
                let n = &graph().nodes[i];
                let st = match n.state {
                    NodeState::Ready => "R",
                    NodeState::Running => "X",
                    NodeState::Exited => "E",
                    NodeState::Waiting => "W",
                };
                s += &alloc::format!("{}  {}  {}  {}\r\n", n.id, n.activation, n.tension, st);
            }
            let mut n = 0usize;
            for b in s.bytes() {
                if n < out_len {
                    unsafe { *out_ptr.add(n) = b };
                    n += 1;
                }
            }
            *(frame_ptr as *mut u64) = n as u64;
            0
        }
        SYS_TOUCH => {
            let path_ptr = *frame.add(5) as *const u8;
            let path_len = *frame.add(4) as usize;
            if !validate_buf(path_ptr, path_len.min(64)) {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = unsafe { *path_ptr.add(i) };
            }
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");
            *(frame_ptr as *mut u64) = fs::touch(path) as u64;
            0
        }
        SYS_MKDIR => {
            let path_ptr = *frame.add(5) as *const u8;
            let path_len = *frame.add(4) as usize;
            if !validate_buf(path_ptr, path_len.min(64)) {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = unsafe { *path_ptr.add(i) };
            }
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");
            *(frame_ptr as *mut u64) = fs::mkdir(path) as u64;
            0
        }
        SYS_WRITE_F => {
            let path_ptr = *frame.add(5) as *const u8;
            let path_len = *frame.add(4) as usize;
            let data_ptr = *frame.add(3) as *const u8;
            let data_len = *frame.add(2) as usize;
            if !validate_buf(path_ptr, path_len.min(64)) || !validate_buf(data_ptr, data_len.min(256)) {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let mut path_buf = [0u8; 64];
            let plen = path_len.min(63);
            for i in 0..plen {
                path_buf[i] = *path_ptr.add(i);
            }
            let path = core::str::from_utf8(&path_buf[..plen]).unwrap_or("");
            let data = core::slice::from_raw_parts(data_ptr, data_len.min(256));
            *(frame_ptr as *mut u64) = fs::write_file(path, data) as u64;
            0
        }
        SYS_SHUTDOWN => {
            do_checkpoint();
            console_write("Checkpoint saved. Halting.\r\n");
            loop {
                asm!("hlt");
            }
        }
        SYS_CLEAR => {
            vga::clear();
            0
        }
        SYS_POLL_KEY => {
            if keyboard::has_key() || serial_has_byte() {
                1
            } else {
                0
            }
        }
        SYS_RM => {
            let path_ptr = *frame.add(5) as *const u8;
            let path_len = *frame.add(4) as usize;
            if !validate_buf(path_ptr, path_len.min(64)) {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = unsafe { *path_ptr.add(i) };
            }
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");
            *(frame_ptr as *mut u64) = fs::rm(path) as u64;
            0
        }
        SYS_GETPID => {
            *(frame_ptr as *mut u64) = if CURRENT_NODE_IDX != 0xFF {
                graph().nodes[CURRENT_NODE_IDX].id as u64
            } else {
                !0u64
            };
            0
        }
        SYS_CHDIR => {
            if CURRENT_NODE_IDX == 0xFF {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let path_ptr = *frame.add(5) as *const u8;
            let path_len = *frame.add(4) as usize;
            if !validate_buf(path_ptr, path_len.min(CWD_MAX)) {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let mut path_buf = [0u8; CWD_MAX];
            let len = path_len.min(CWD_MAX - 1);
            for i in 0..len {
                path_buf[i] = unsafe { *path_ptr.add(i) };
            }
            path_buf[len] = 0;
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("/");
            if path == "/" || fs::path_is_dir(path) {
                let node = &mut graph().nodes[CURRENT_NODE_IDX];
                let copy = path.len().min(CWD_MAX - 1);
                for i in 0..copy {
                    node.cwd[i] = path.as_bytes()[i];
                }
                node.cwd[copy] = 0;
                *(frame_ptr as *mut u64) = 1;
            } else {
                *(frame_ptr as *mut u64) = 0;
            }
            0
        }
        SYS_GETCWD => {
            if CURRENT_NODE_IDX == 0xFF {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let out_ptr = *frame.add(5) as *mut u8;
            let out_len = *frame.add(4) as usize;
            if !validate_buf(out_ptr, out_len) {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let node = &graph().nodes[CURRENT_NODE_IDX];
            let mut len = 0;
            while len < CWD_MAX && node.cwd[len] != 0 {
                len += 1;
            }
            let copy = len.min(out_len.saturating_sub(1));
            for i in 0..copy {
                unsafe { *out_ptr.add(i) = node.cwd[i] };
            }
            if copy < out_len {
                unsafe { *out_ptr.add(copy) = 0 };
            }
            *(frame_ptr as *mut u64) = copy as u64;
            0
        }
        SYS_WAIT => {
            if CURRENT_NODE_IDX == 0xFF {
                *(frame_ptr as *mut u64) = !0u64;
                return 0;
            }
            let cur_idx = CURRENT_NODE_IDX;
            let mut exited_child = None;
            for i in 0..graph().count() {
                if graph().nodes[i].parent == cur_idx && matches!(graph().nodes[i].state, NodeState::Exited) {
                    exited_child = Some(i);
                    break;
                }
            }
            if let Some(idx) = exited_child {
                let status = graph().nodes[idx].exit_status;
                let pid = graph().nodes[idx].id;
                graph().nodes[idx].parent = PARENT_NONE;
                *(frame_ptr as *mut u64) = (pid as u64) << 8 | status as u64;
                0
            } else {
                graph().nodes[cur_idx].state = NodeState::Waiting;
                graph().nodes[cur_idx].saved_rsp = frame_ptr as u64;
                let cur = Some(cur_idx);
                do_schedule(cur)
            }
        }
        SYS_KILL => {
            let pid = *frame.add(5) as u32;
            let sig = *frame.add(4) as u32;
            if sig == 9 {
                for i in 0..graph().count() {
                    if graph().nodes[i].id == pid {
                        graph().nodes[i].state = NodeState::Exited;
                        graph().nodes[i].exit_status = 128 + 9;
                        let parent = graph().nodes[i].parent;
                        if parent != PARENT_NONE && parent < graph().count() {
                            if matches!(graph().nodes[parent].state, NodeState::Waiting) {
                                graph().nodes[parent].state = NodeState::Ready;
                                let parent_rsp = graph().nodes[parent].saved_rsp;
                                if parent_rsp != 0 {
                                    unsafe {
                                        *(parent_rsp as *mut u64) = (pid as u64) << 8 | 137;
                                    }
                                }
                            }
                        }
                        *(frame_ptr as *mut u64) = 0;
                        return 0;
                    }
                }
            }
            *(frame_ptr as *mut u64) = !0u64;
            0
        }
        _ => !0u64,
    }
}

#[inline(never)]
unsafe extern "C" fn node_entry() {
    loop {
        let idx = CURRENT_NODE_IDX;
        if idx == 0xFF {
            break;
        }
        let node_id = graph().nodes[idx].id;
        let act = graph().nodes[idx].activation;
        match node_id {
            0 => {
                do_sys_write(b"Node 0 stats: act=");
                let s = alloc::format!("{}\r\n", act);
                do_sys_write(s.as_bytes());
            }
            1 => do_sys_write(b"Node 1 alive\r\n"),
            4 => do_sys_write(b"Node 4 working\r\n"),
            _ => {
                do_sys_write(b"Node ");
                let s = alloc::format!("{} tick\r\n", node_id);
                do_sys_write(s.as_bytes());
            }
        }
        small_delay();
        do_sys_yield();
    }
}

fn init_node_stacks(g: &mut ProcessGraph) {
    for i in 0..g.count() {
        let entry = if g.nodes[i].id == 0 {
            shell::shell_main as *const () as u64
        } else {
            node_entry as *const () as u64
        };
        init_node_stacks_for_with_entry(&mut g.nodes[i], entry);
    }
}

fn init_node_stacks_for(node: &mut ProcessNode) {
    let entry = node_entry as *const () as u64;
    init_node_stacks_for_with_entry(node, entry);
}

const INIT_FRAME_SIZE: usize = 168;

fn init_node_stacks_for_with_entry(node: &mut ProcessNode, entry: u64) {
    let (code_sel, data_sel) = if node.id == 0 {
        (0x08u64, 0x10u64)
    } else {
        (0x1bu64, 0x23u64)
    };
    let rflags = 0x202u64;
    unsafe {
        node.cr3 = if HHDM_OFFSET != 0 {
            paging::create_process_page_table(HHDM_OFFSET).unwrap_or(KERNEL_CR3)
        } else {
            KERNEL_CR3
        };
        let base = node.stack.as_mut_ptr().add(STACK_SIZE - INIT_FRAME_SIZE) as *mut u64;
        let stack_rsp = base as u64;
        for j in 0..16 {
            base.add(j).write(0);
        }
        base.add(16).write(entry);
        base.add(17).write(code_sel);
        base.add(18).write(rflags);
        base.add(19).write(stack_rsp);
        base.add(20).write(data_sel);
    }
}

const VGA_TEXT_BASE: u64 = 0xB8000;

/// Safe VGA text at 0xB8000: clear + print using 16-bit cells (0x0F00 | char).
fn safe_vga_text_clear_and_print(framebuffer: u64) {
    if framebuffer == 0 || framebuffer < 0x1000 {
        serial_write("INVALID FB ADDR\r\n");
        loop {
            unsafe { asm!("hlt") };
        }
    }
    let ptr = framebuffer as *mut u16;
    unsafe {
        for i in 0..(80 * 25) {
            *ptr.add(i) = 0x0F00 | b' ' as u16;
        }
        let s = b"TS-OS Strongest Node online";
        for (i, &b) in s.iter().enumerate() {
            if i < 80 {
                *ptr.add(i) = 0x0F00 | (b as u16);
            }
        }
    }
}

/// Safe Limine framebuffer clear (pixel buffer to black). Returns true if write succeeded.
fn safe_limine_fb_clear(addr: u64, pitch: u64, height: u64) -> bool {
    if addr == 0 || addr < 0x1000 {
        return false;
    }
    let total_bytes = (pitch * height) as usize;
    let ptr = addr as *mut u8;
    unsafe {
        for i in 0..total_bytes {
            *ptr.add(i) = 0;
        }
    }
    true
}

/// Full boot sequence: Limine, GDT/IDT/TSS, VGA, fs, persist, graph, shell.
fn full_boot() -> ! {
    serial_init();
    serial_write("K1 - Kernel entry\r\n");

    // Init kernel heap (must be before any allocation)
    allocator::init();

    // Limine: check revision
    if !BASE_REVISION.is_supported() {
        serial_write("Limine rev unsupported\r\n");
        hcf();
    }

    // HHDM for higher-half mapping
    let hhdm = HHDM_REQUEST.get_response().expect("HHDM response");
    unsafe {
        HHDM_OFFSET = hhdm.offset();
    }
    unsafe {
        asm!("mov {}, cr3", out(reg) KERNEL_CR3, options(nostack, preserves_flags));
    }

    // VGA: framebuffer or text mode
    if let Some(fb_resp) = FRAMEBUFFER_REQUEST.get_response() {
        if let Some(fb) = fb_resp.framebuffers().next() {
            let addr = fb.addr();
            let w = fb.width();
            let h = fb.height();
            let pitch = fb.pitch();
            let bpp = fb.bpp();
            if !addr.is_null() && w >= 640 && h >= 400 && bpp >= 24 {
                vga::init_framebuffer(addr, w, h, pitch, bpp);
            }
        }
    }
    if !vga::is_enabled() {
        vga::init_text_mode(unsafe { HHDM_OFFSET });
    }

    // Initialize physical frame allocator from Limine memory map
    let mmap = MEMORY_MAP_REQUEST.get_response().expect("Memory map response");
    let entries = mmap.entries();
    let alloc = memory::frame_allocator::BitmapFrameAllocator::init(entries, unsafe { &mut FRAME_BITMAP.0 })
        .expect("BitmapFrameAllocator init");
    unsafe {
        paging::init_frame_allocator(alloc);
    }

    // GDT + TSS
    let mut tss = TaskStateSegment::new();
    let df_stack_top = VirtAddr::new(unsafe { &DOUBLE_FAULT_STACK as *const _ as u64 } + 4096);
    tss.interrupt_stack_table[1] = df_stack_top;
    // IST[0] used as per-process kernel stack for interrupts from user mode.
    let ist0_top = VirtAddr::new(unsafe { &KERNEL_IST_STACKS[0] as *const _ as u64 } + 4096);
    tss.interrupt_stack_table[0] = ist0_top;

    unsafe {
        TSS.write(tss);
    }

    let mut gdt = GlobalDescriptorTable::new();
    let _ = gdt.add_entry(Descriptor::kernel_code_segment());
    let _ = gdt.add_entry(Descriptor::kernel_data_segment());
    let _ = gdt.add_entry(Descriptor::user_code_segment());
    let _ = gdt.add_entry(Descriptor::user_data_segment());
    let tss_sel = gdt.add_entry(Descriptor::tss_segment(unsafe { &*TSS.as_ptr() }));

    unsafe {
        let gdt_leak = Box::leak(Box::new(gdt));
        gdt_leak.load();
        SS::set_reg(SegmentSelector::new(2, PrivilegeLevel::Ring0));
        CS::set_reg(SegmentSelector::new(1, PrivilegeLevel::Ring0));
        load_tss(tss_sel);
    }

    // IDT
    let mut idt = InterruptDescriptorTable::new();
    let df_handler = double_fault_stub as *const () as u64;
    unsafe {
        idt.double_fault
            .set_handler_addr(VirtAddr::new(df_handler))
            .set_stack_index(1);
    }
    idt[IRQ_TIMER as usize]
        .set_handler_fn(timer_handler_x86);
    unsafe {
        idt[SYSCALL_VECTOR as usize]
            .set_handler_addr(VirtAddr::new(syscall_stub as *const () as u64))
            .set_privilege_level(PrivilegeLevel::Ring3);
    }
    idt.page_fault.set_handler_fn(page_fault_handler);
    let idt_leak = Box::leak(Box::new(idt));
    idt_leak.load();

    // PIC, PIT, keyboard
    unsafe {
        pic_remap();
        pit_init();
    }
    keyboard::init();

    // FS
    fs::init();

    // Persist restore (with crash recovery: validate before using)
    let restored = persist::try_restore();
    let valid = restored && persist::validate_current_checkpoint();
    if valid {
        let mut fs_buf = [0u8; 16384];
        let f_len = persist::restore_fs(fs_buf.as_mut_ptr(), 16384);
        if f_len > 0 {
            let _ = fs::deserialize_from(&fs_buf[..f_len]);
        }
    }

    // Process graph
    unsafe {
        GRAPH = Some(ProcessGraph::new());
    }
    if graph().count() == 0 {
        graph().add_node(0, 100, 0, &[], PARENT_NONE);
    }

    // Default VMAs for newly spawned user nodes (Phase 1 baseline):
    // - code/data: 0x0040_0000..0x0080_0000
    // - stack: (stack_top-8MiB)..stack_top, with guard at stack_top-8MiB..stack_top-8MiB+4KiB left unmapped
    // Node 0 is kernel-mode shell; VMAs are not used.
    for n in graph().nodes.iter_mut() {
        if n.id == 0 {
            continue;
        }
        n.vma_count = 0;
        let code = Vma { start: 0x0000_0000_0040_0000, end: 0x0000_0000_0080_0000, writable: true, executable: true };
        let stack_top = 0x0000_7FFF_FFFF_F000u64;
        let stack = Vma { start: stack_top - (8 * 1024 * 1024), end: stack_top, writable: true, executable: false };
        n.vmas[0] = code;
        n.vmas[1] = stack;
        n.vma_count = 2;
    }
    init_node_stacks(graph());

    // Save kernel RSP and switch to shell
    let shell_rsp = (graph().nodes[0].stack.as_ptr() as u64) + (STACK_SIZE - INIT_FRAME_SIZE) as u64;
    unsafe {
        asm!(
            "mov {}, rsp",
            out(reg) KERNEL_RSP,
            options(nostack, preserves_flags)
        );
        bootstrap_switch(shell_rsp, &mut KERNEL_RSP);
    }
}

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    full_boot();
}

#[panic_handler]
fn rust_panic(_info: &core::panic::PanicInfo) -> ! {
    console_write("PANIC\r\n");
    hcf();
}

pub(crate) fn hcf() -> ! {
    loop {
        unsafe { asm!("hlt") };
    }
}
