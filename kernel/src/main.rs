//! TS-OS Kernel — Strongest Node Framework
//!
//! Kernel = core_engine (structural search, pattern discovery, constraint resolution)
//! Scheduler = pure Strongest Node weighted spreading activation
//! Real context switch: save/restore RSP, yield_to_kernel from node entries

#![no_std]
#![no_main]

use core::arch::asm;

use limine::request::{RequestsEndMarker, RequestsStartMarker};
use limine::BaseRevision;

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();
#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

const COM1: u16 = 0x3F8;
const LCR: u16 = COM1 + 3;
const DLL: u16 = COM1 + 0;
const DLM: u16 = COM1 + 1;
const FCR: u16 = COM1 + 2;

const MAX_NODES: usize = 8;
const MAX_NEIGHBORS: usize = 4;
const NEIGHBOR_NONE: u8 = 0xFF;
const STACK_SIZE: usize = 4096;

/// ~100ms busy loop (QEMU: ~50–100ms depending on host)
const TICK_LOOPS: u64 = 80_000_000;

static mut KERNEL_RSP: u64 = 0;
static mut CURRENT_NODE_IDX: usize = 0;

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

#[derive(Clone, Copy)]
#[repr(u8)]
enum NodeState {
    Ready = 0,
    Running = 1,
}

/// Process graph node: id, activation, tension, state, neighbors, stack, saved context
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

/// In-RAM process graph (static array, no heap)
struct ProcessGraph {
    nodes: [ProcessNode; MAX_NODES],
    count: usize,
}

impl ProcessGraph {
    const fn new() -> Self {
        Self {
            nodes: [ProcessNode::empty(); MAX_NODES],
            count: 0,
        }
    }

    fn add_node(&mut self, id: u32, activation: u32, tension: u32, neighbors: &[u8]) -> bool {
        if self.count >= MAX_NODES {
            return false;
        }
        let i = self.count;
        self.nodes[i] = ProcessNode {
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
        };
        self.count += 1;
        true
    }

    fn decay_all(&mut self, except: usize) {
        for i in 0..self.count {
            if i != except {
                self.nodes[i].activation = self.nodes[i].activation.saturating_sub(2);
            }
        }
    }

    fn spread_from(&mut self, from: usize) {
        const SPREAD: u32 = 10;
        for &idx in &self.nodes[from].neighbors {
            if idx == NEIGHBOR_NONE {
                break;
            }
            let i = idx as usize;
            if i < self.count && i != from {
                self.nodes[i].activation = self.nodes[i].activation.saturating_add(SPREAD);
                if self.nodes[i].activation > 200 {
                    self.nodes[i].activation = 200;
                }
            }
        }
    }

    fn select_strongest(&self) -> Option<usize> {
        if self.count == 0 {
            return None;
        }
        let mut best = 0;
        let mut best_s = self.nodes[0].effective_strength();
        for i in 1..self.count {
            let s = self.nodes[i].effective_strength();
            if s > best_s {
                best_s = s;
                best = i;
            }
        }
        Some(best)
    }
}

static mut GRAPH: ProcessGraph = ProcessGraph::new();

fn busy_wait(loops: u64) {
    let mut i = 0u64;
    while i < loops {
        core::hint::spin_loop();
        i += 1;
    }
}

fn small_delay() {
    for _ in 0..1000 {
        core::hint::spin_loop();
    }
}

/// Switch to node: save kernel RSP, set RSP to node stack, jump to entry. Resumes if saved_rsp != 0.
#[inline(never)]
unsafe fn switch_to_node(idx: usize) {
    asm!("mov {}, rsp", out(reg) KERNEL_RSP, options(nostack, preserves_flags));
    CURRENT_NODE_IDX = idx;
    let node = &mut GRAPH.nodes[idx];
    if node.saved_rsp != 0 {
        asm!(
            "mov rsp, {}",
            "jmp {}",
            in(reg) node.saved_rsp,
            in(reg) node.saved_rip,
            options(nostack, preserves_flags)
        );
    } else {
        let rsp_val = node.stack.as_ptr().add(STACK_SIZE - 8) as u64;
        asm!(
            "mov rsp, {}",
            "ret",
            in(reg) rsp_val,
            options(nostack, preserves_flags)
        );
    }
}

/// Yield: save node context, restore kernel RSP, return to scheduler
#[inline(never)]
#[no_mangle]
unsafe extern "C" fn yield_to_kernel() {
    let idx = CURRENT_NODE_IDX;
    let node = &mut GRAPH.nodes[idx];
    let rsp: u64;
    let rip: u64;
    asm!("mov {}, rsp", out(reg) rsp, options(nostack, preserves_flags));
    asm!("mov {}, [rsp]", out(reg) rip, options(nostack, preserves_flags));
    node.saved_rsp = rsp;
    node.saved_rip = rip;
    asm!("mov rsp, {}", in(reg) KERNEL_RSP, options(nostack, preserves_flags));
}

/// Node entry: infinite loop, do work, yield. Dispatches by CURRENT_NODE_IDX.
#[inline(never)]
unsafe extern "C" fn node_entry() {
    loop {
        let idx = CURRENT_NODE_IDX;
        let node_id = GRAPH.nodes[idx].id;
        let act = GRAPH.nodes[idx].activation;
        match node_id {
            0 => {
                serial_write("Node 0 stats: act=");
                serial_write_u32(act);
                serial_write("\r\n");
            }
            1 => serial_write("Node 1 alive\r\n"),
            4 => serial_write("Node 4 working\r\n"),
            _ => {
                serial_write("Node ");
                serial_write_u32(node_id);
                serial_write(" tick\r\n");
            }
        }
        small_delay();
        yield_to_kernel();
    }
}

/// Initialize node stack: write entry point at top so `ret` jumps to node_entry
fn init_node_stacks(g: &mut ProcessGraph) {
    let entry_addr = node_entry as *const () as u64;
    for i in 0..g.count {
        let slot = g.nodes[i].stack.as_mut_ptr().add(STACK_SIZE - 8) as *mut u64;
        unsafe { slot.write(entry_addr) };
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    let _ = &BASE_REVISION;
    let _ = &_START_MARKER;
    let _ = &_END_MARKER;
    assert!(BASE_REVISION.is_supported());

    serial_init();
    serial_write("TS-OS Strongest Node online\r\n");

    let g = &mut GRAPH;
    g.add_node(0, 100, 0, &[1, 3]);
    g.add_node(1, 80, 10, &[0, 2]);
    g.add_node(2, 60, 5, &[1, 3]);
    g.add_node(3, 40, 20, &[2, 0]);
    g.add_node(4, 80, 0, &[0]);

    init_node_stacks(g);
    serial_write("Process graph: 5 nodes, real context switch, yield_to_kernel\r\n");

    let mut prev_strongest: Option<usize> = None;

    for tick_num in 0..8u32 {
        busy_wait(TICK_LOOPS);

        let current = g.select_strongest();
        if let Some(cur) = current {
            g.decay_all(cur);
            g.spread_from(cur);
            let new_strongest = g.select_strongest().unwrap();
            let id = g.nodes[new_strongest].id;
            let act = g.nodes[new_strongest].activation;
            g.nodes[new_strongest].state = NodeState::Running;

            let do_switch = prev_strongest != Some(new_strongest);
            prev_strongest = Some(new_strongest);

            if do_switch {
                serial_write("tick ");
                serial_write_u32(tick_num);
                serial_write(": switch to node ");
                serial_write_u32(id);
                serial_write(" (act=");
                serial_write_u32(act);
                serial_write(")\r\n");

                switch_to_node(new_strongest);
            }
        }
    }

    serial_write("TS-OS tick loop complete. HCF.\r\n");

    hcf();
}

#[panic_handler]
fn rust_panic(_info: &core::panic::PanicInfo) -> ! {
    serial_write("PANIC\r\n");
    hcf();
}

fn hcf() -> ! {
    loop {
        unsafe { asm!("hlt") };
    }
}
