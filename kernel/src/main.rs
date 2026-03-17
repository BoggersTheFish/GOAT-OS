//! TS-OS Kernel — Strongest Node Framework
//!
//! Kernel = core_engine (structural search, pattern discovery, constraint resolution)
//! Scheduler = pure Strongest Node weighted spreading activation
//! PIT timer interrupt + preemptive scheduler + bump heap

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![allow(dead_code, static_mut_refs)]

extern crate alloc;

mod allocator;
mod fs;
mod keyboard;
mod persist;
mod shell;
mod vga;

use alloc::string::ToString;
use core::alloc::Layout;
use core::arch::asm;
use core::mem::MaybeUninit;

use limine::request::{FramebufferRequest, HhdmRequest, RequestsEndMarker, RequestsStartMarker};
use limine::BaseRevision;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable};
use x86_64::structures::idt::InterruptDescriptorTable;
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
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

const COM1: u16 = 0x3F8;
const LCR: u16 = COM1 + 3;
const DLL: u16 = COM1 + 0;
const DLM: u16 = COM1 + 1;
const FCR: u16 = COM1 + 2;

const PIT_CH0: u16 = 0x40;
const PIT_CMD: u16 = 0x43;
const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const MAX_NODES: usize = 32;
const MAX_NEIGHBORS: usize = 4;
const NEIGHBOR_NONE: u8 = 0xFF;
const STACK_SIZE: usize = 4096;
const IRQ_TIMER: u8 = 32;
const SYSCALL_VECTOR: u8 = 0x80;
const KERNEL_STACK_SIZE: usize = 4096;

static mut KERNEL_RSP: u64 = 0;
static mut CURRENT_NODE_IDX: usize = 0xFF;
static mut TICK_COUNT: u32 = 0;

core::arch::global_asm!(
    r#"
    .text
    .global timer_stub
    .global syscall_stub

timer_stub:
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
    call timer_handler
    cmp rax, 0
    jz .no_switch_timer
    mov rsp, rax
.no_switch_timer:
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
"#
);

extern "C" {
    fn timer_stub();
    fn syscall_stub();
}

#[repr(align(16))]
struct KernelStack([u8; KERNEL_STACK_SIZE]);
static mut KERNEL_STACK: KernelStack = KernelStack([0; KERNEL_STACK_SIZE]);
static mut TSS: MaybeUninit<TaskStateSegment> = MaybeUninit::uninit();

#[global_allocator]
static HEAP: allocator::BumpAllocator = allocator::BumpAllocator;

#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    serial_write("ALLOC ERR\r\n");
    hcf();
}

#[inline(always)]
unsafe fn outb(port: u16, byte: u8) {
    asm!("out dx, al", in("dx") port, in("al") byte, options(nostack, preserves_flags));
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

fn serial_write(s: &str) {
    for b in s.bytes() {
        unsafe { outb(COM1, b) };
    }
}

fn console_write(s: &str) {
    serial_write(s);
    vga::write_str(s);
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

#[derive(Clone, Copy)]
#[repr(u8)]
enum NodeState {
    Ready = 0,
    Running = 1,
    Exited = 2,
}

struct ProcessNode {
    id: u32,
    activation: u32,
    tension: u32,
    state: NodeState,
    neighbors: [u8; MAX_NEIGHBORS],
    stack: [u8; STACK_SIZE],
    saved_rip: u64,
    saved_rsp: u64,
}

impl ProcessNode {
    const fn empty() -> Self {
        Self {
            id: 0,
            activation: 0,
            tension: 0,
            state: NodeState::Ready,
            neighbors: [NEIGHBOR_NONE; MAX_NEIGHBORS],
            stack: [0u8; STACK_SIZE],
            saved_rip: 0,
            saved_rsp: 0,
        }
    }

    fn effective_strength(&self) -> i32 {
        self.activation as i32 - self.tension as i32
    }
}

struct ProcessGraph {
    nodes: alloc::vec::Vec<ProcessNode>,
}

impl ProcessGraph {
    fn new() -> Self {
        Self {
            nodes: alloc::vec::Vec::new(),
        }
    }

    fn count(&self) -> usize {
        self.nodes.len()
    }

    fn add_node(&mut self, id: u32, activation: u32, tension: u32, neighbors: &[u8]) -> bool {
        if self.nodes.len() >= MAX_NODES {
            return false;
        }
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
        });
        true
    }

    fn decay_all(&mut self, except: usize) {
        for i in 0..self.nodes.len() {
            if i != except {
                self.nodes[i].activation = self.nodes[i].activation.saturating_sub(2);
            }
        }
    }

    fn spread_from(&mut self, from: usize) {
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

    fn select_strongest(&self) -> Option<usize> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut best = None;
        let mut best_s = i32::MIN;
        for i in 0..self.nodes.len() {
            if matches!(self.nodes[i].state, NodeState::Exited) {
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

    fn try_add_node(&mut self, id: u32, activation: u32, tension: u32, neighbors: &[u8]) -> Option<usize> {
        if self.nodes.len() >= MAX_NODES {
            return None;
        }
        let i = self.nodes.len();
        self.add_node(id, activation, tension, neighbors);
        Some(i)
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

fn do_checkpoint() {
    let mut graph_buf = [0u8; 64];
    graph_buf[0] = graph().count() as u8;
    let mut fs_buf = [0u8; 16384];
    let fs_len = fs::serialize_to(&mut fs_buf);
    persist::do_checkpoint(graph_buf.as_ptr(), 64, fs_buf.as_ptr(), fs_len);
}

fn maybe_emerge_node() {
    let mut max_tension = 0u32;
    for i in 0..graph().count() {
        if !matches!(graph().nodes[i].state, NodeState::Exited) {
            max_tension = max_tension.max(graph().nodes[i].tension);
        }
    }
    if max_tension > 30 && graph().count() < MAX_NODES {
        let next_id = graph().count() as u32;
        if let Some(idx) = graph().try_add_node(next_id, 50, 0, &[]) {
            init_node_stacks_for(&mut graph().nodes[idx]);
            graph().nodes[idx].state = NodeState::Ready;
        }
    }
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
            unsafe { CURRENT_NODE_IDX = ns };
            if graph().nodes[ns].saved_rsp != 0 {
                return graph().nodes[ns].saved_rsp;
            }
            return unsafe { graph().nodes[ns].stack.as_ptr().add(STACK_SIZE - 160) as u64 };
        }
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
            for i in 0..len {
                let b = *buf.add(i);
                outb(COM1, b);
                vga::write_byte(b);
            }
            *(frame_ptr as *mut u64) = len as u64;
            0
        }
        SYS_READ => {
            let fd = *frame.add(5);
            let buf = *frame.add(4) as *mut u8;
            let len = *frame.add(3) as usize;
            let mut n = 0usize;
            if fd == 0 && len > 0 {
                if let Some(c) = keyboard::read_byte() {
                    *buf.add(0) = c;
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
            if let Some(idx) = graph().try_add_node(next_id, 50, 0, &[]) {
                init_node_stacks_for(&mut graph().nodes[idx]);
                graph().nodes[idx].state = NodeState::Ready;
                *frame_mut.add(0) = idx as u64;
            } else {
                *frame_mut.add(0) = !0u64;
            }
            0
        }
        SYS_EXIT => {
            let _status = *frame.add(5);
            if CURRENT_NODE_IDX != 0xFF {
                graph().nodes[CURRENT_NODE_IDX].state = NodeState::Exited;
                graph().nodes[CURRENT_NODE_IDX].saved_rsp = frame_ptr as u64;
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
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = *path_ptr.add(i);
            }
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("/");
            let entries = fs::list_dir(path);
            let mut n = 0usize;
            for e in &entries {
                let s = alloc::format!("{}\r\n", e);
                for b in s.bytes() {
                    if n < out_len {
                        *out_ptr.add(n) = b;
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
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = *path_ptr.add(i);
            }
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");
            let content = fs::cat(path).unwrap_or_else(|| "".to_string());
            let mut n = 0usize;
            for b in content.bytes() {
                if n < out_len {
                    *out_ptr.add(n) = b;
                    n += 1;
                }
            }
            *(frame_ptr as *mut u64) = n as u64;
            0
        }
        SYS_PS => {
            let out_ptr = *frame.add(5) as *mut u8;
            let out_len = *frame.add(4) as usize;
            let mut s = alloc::format!("pid  act  ten  state\r\n");
            for i in 0..graph().count() {
                let n = &graph().nodes[i];
                let st = match n.state {
                    NodeState::Ready => "R",
                    NodeState::Running => "X",
                    NodeState::Exited => "E",
                };
                s += &alloc::format!("{}  {}  {}  {}\r\n", n.id, n.activation, n.tension, st);
            }
            let mut n = 0usize;
            for b in s.bytes() {
                if n < out_len {
                    *out_ptr.add(n) = b;
                    n += 1;
                }
            }
            *(frame_ptr as *mut u64) = n as u64;
            0
        }
        SYS_TOUCH => {
            let path_ptr = *frame.add(5) as *const u8;
            let path_len = *frame.add(4) as usize;
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = *path_ptr.add(i);
            }
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");
            *(frame_ptr as *mut u64) = fs::touch(path) as u64;
            0
        }
        SYS_MKDIR => {
            let path_ptr = *frame.add(5) as *const u8;
            let path_len = *frame.add(4) as usize;
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = *path_ptr.add(i);
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
            if keyboard::has_key() { 1 } else { 0 }
        }
        _ => !0u64,
    }
}

#[no_mangle]
pub unsafe extern "C" fn timer_handler(frame_ptr: *mut u8) -> u64 {
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

    if let Some(idx) = cur {
        graph().nodes[idx].saved_rsp = frame_ptr as u64;
        graph().nodes[idx].tension = graph().nodes[idx].tension.saturating_add(1);
    } else {
        KERNEL_RSP = frame_ptr as u64 + 120;
    }

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
            unsafe { CURRENT_NODE_IDX = ns };
            if graph().nodes[ns].saved_rsp != 0 {
                return graph().nodes[ns].saved_rsp;
            }
            return unsafe { graph().nodes[ns].stack.as_ptr().add(STACK_SIZE - 160) as u64 };
        }
    }

    0
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

fn init_node_stacks_for_with_entry(node: &mut ProcessNode, entry: u64) {
    let code_sel = 0x1bu64;
    let data_sel = 0x23u64;
    let rflags = 0x202u64;
    unsafe {
        let base = node.stack.as_mut_ptr().add(STACK_SIZE - 160) as *mut u64;
        let stack_rsp = base as u64;
        for j in 0..15 {
            base.add(j).write(0);
        }
        base.add(15).write(entry);
        base.add(16).write(code_sel);
        base.add(17).write(rflags);
        base.add(18).write(stack_rsp);
        base.add(19).write(data_sel);
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    let _ = &BASE_REVISION;
    let _ = &_START_MARKER;
    let _ = &_END_MARKER;
    assert!(BASE_REVISION.is_supported());

    allocator::init();
    serial_init();
    vga::init();
    if let Some(fb_resp) = FRAMEBUFFER_REQUEST.get_response() {
        if let Some(fb) = fb_resp.framebuffers().next() {
            let raw = fb.addr() as u64;
            let addr = if raw < 0x1_0000_0000 {
                HHDM_REQUEST
                    .get_response()
                    .map(|h| raw + h.offset())
                    .unwrap_or(raw)
            } else {
                raw
            };
            serial_write("FB: ");
            serial_write_hex(raw);
            serial_write(" -> ");
            serial_write_hex(addr);
            serial_write(" ");
            serial_write_u32(fb.width() as u32);
            serial_write("x");
            serial_write_u32(fb.height() as u32);
            serial_write("\r\n");
            vga::init_framebuffer(addr as *mut u8, fb.width(), fb.height(), fb.pitch(), fb.bpp());
        } else {
            serial_write("FB: no framebuffers\r\n");
        }
    } else {
        serial_write("FB: no response\r\n");
    }
    fs::init();
    let mut restored = false;
    if persist::try_restore() {
        let mut fs_buf = [0u8; 16384];
        let n = persist::restore_fs(fs_buf.as_mut_ptr(), fs_buf.len());
        if n > 0 && fs::deserialize_from(&fs_buf[..n]) {
            restored = true;
            console_write("Restored filesystem from checkpoint\r\n");
        }
    }
    if !restored {
        fs::mkdir("/tmp");
        fs::touch("/readme.txt");
        fs::write_file("/readme.txt", b"TS-OS - Strongest Node Framework. Type 'help' in shell.\r\n");
    }
    console_write("TS-OS Strongest Node online\r\n");

    let mut gdt = GlobalDescriptorTable::new();
    gdt.add_entry(Descriptor::kernel_code_segment());
    gdt.add_entry(Descriptor::kernel_data_segment());
    gdt.add_entry(Descriptor::user_code_segment());
    gdt.add_entry(Descriptor::user_data_segment());
    let tss = TSS.write(TaskStateSegment::new());
    tss.privilege_stack_table[0] = VirtAddr::from_ptr(
        KERNEL_STACK.0.as_ptr().add(KERNEL_STACK_SIZE) as *const u8,
    );
    let tss_sel = gdt.add_entry(Descriptor::tss_segment(unsafe { TSS.assume_init_ref() }));
    let gdt = alloc::boxed::Box::leak(alloc::boxed::Box::new(gdt));
    gdt.load();
    unsafe { x86_64::instructions::tables::load_tss(tss_sel) };

    let mut idt = InterruptDescriptorTable::new();
    unsafe {
        idt[IRQ_TIMER as usize].set_handler_addr(VirtAddr::from_ptr(timer_stub as *const ()));
        let opt = idt[SYSCALL_VECTOR as usize].set_handler_addr(VirtAddr::from_ptr(syscall_stub as *const ()));
        opt.set_privilege_level(PrivilegeLevel::Ring3);
    }
    let idt = alloc::boxed::Box::leak(alloc::boxed::Box::new(idt));
    idt.load();

    pic_remap();
    pit_init();

    unsafe { GRAPH = Some(ProcessGraph::new()) };
    let g = graph();
    g.add_node(0, 100, 0, &[1, 3]);
    g.add_node(1, 80, 10, &[0, 2]);
    g.add_node(2, 60, 5, &[1, 3]);
    g.add_node(3, 40, 20, &[2, 0]);
    g.add_node(4, 80, 0, &[0]);

    init_node_stacks(g);
    console_write("Process graph: 5 nodes, PIT preemption, yield_to_kernel\r\n");

    let buf = alloc::vec::Vec::from(b"Heap OK\r\n");
    for &b in &buf {
        vga::write_byte(b);
        outb(COM1, b);
    }

    asm!("sti");

    loop {
        asm!("hlt");
    }
}

#[panic_handler]
fn rust_panic(_info: &core::panic::PanicInfo) -> ! {
    console_write("PANIC\r\n");
    hcf();
}

fn hcf() -> ! {
    loop {
        unsafe { asm!("hlt") };
    }
}
