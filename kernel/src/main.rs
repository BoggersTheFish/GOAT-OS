//! TS-OS Kernel — Strongest Node Framework
//!
//! Kernel = core_engine (structural search, pattern discovery, constraint resolution)
//! Scheduler = pure Strongest Node weighted spreading activation
//! PIT timer interrupt + preemptive scheduler (replaces busy-wait)

#![no_std]
#![no_main]

use core::arch::asm;

use limine::request::{RequestsEndMarker, RequestsStartMarker};
use limine::BaseRevision;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable};
use x86_64::structures::idt::InterruptDescriptorTable;
use x86_64::VirtAddr;

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

const PIT_CH0: u16 = 0x40;
const PIT_CMD: u16 = 0x43;
const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const MAX_NODES: usize = 8;
const MAX_NEIGHBORS: usize = 4;
const NEIGHBOR_NONE: u8 = 0xFF;
const STACK_SIZE: usize = 4096;
const IRQ_TIMER: u8 = 32;

static mut KERNEL_RSP: u64 = 0;
static mut CURRENT_NODE_IDX: usize = 0xFF;
static mut TICK_COUNT: u32 = 0;

extern "C" {
    fn timer_stub();
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

fn small_delay() {
    for _ in 0..1000 {
        core::hint::spin_loop();
    }
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
    let node = &mut GRAPH.nodes[idx];
    let rsp: u64;
    let rip: u64;
    asm!("mov {}, rsp", out(reg) rsp, options(nostack, preserves_flags));
    asm!("mov {}, [rsp]", out(reg) rip, options(nostack, preserves_flags));
    node.saved_rsp = rsp;
    node.saved_rip = rip;
    asm!("mov rsp, {}", in(reg) KERNEL_RSP, options(nostack, preserves_flags));
}

#[no_mangle]
pub unsafe extern "C" fn timer_handler(frame_ptr: *mut u8) -> u64 {
    outb(PIC1_CMD, 0x20);

    TICK_COUNT += 1;

    let cur = if CURRENT_NODE_IDX != 0xFF {
        Some(CURRENT_NODE_IDX)
    } else {
        None
    };

    if let Some(idx) = cur {
        GRAPH.nodes[idx].saved_rsp = frame_ptr as u64;
    } else {
        KERNEL_RSP = frame_ptr as u64 + 120;
    }

    let current = GRAPH.select_strongest();
    if let Some(c) = current {
        GRAPH.decay_all(c);
        GRAPH.spread_from(c);
    }

    let new_strongest = GRAPH.select_strongest();
    if let Some(ns) = new_strongest {
        GRAPH.nodes[ns].state = NodeState::Running;
        let do_switch = cur != Some(ns) || cur.is_none();
        if do_switch {
            CURRENT_NODE_IDX = ns;
            if GRAPH.nodes[ns].saved_rsp != 0 {
                return GRAPH.nodes[ns].saved_rsp;
            }
            return GRAPH.nodes[ns].stack.as_ptr().add(STACK_SIZE - 160) as u64;
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

fn init_node_stacks(g: &mut ProcessGraph) {
    let entry = node_entry as *const () as u64;
    let code_sel = 0x08u64;
    let data_sel = 0x10u64;
    let rflags = 0x202u64;
    for i in 0..g.count {
        let base = g.nodes[i].stack.as_mut_ptr().add(STACK_SIZE - 160) as *mut u64;
        let base_addr = base as u64;
        for j in 0..15 {
            base.add(j).write(0);
        }
        base.add(15).write(entry);
        base.add(16).write(code_sel);
        base.add(17).write(rflags);
        base.add(18).write(base_addr);
        base.add(19).write(data_sel);
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

    let mut gdt = GlobalDescriptorTable::new();
    gdt.append(Descriptor::kernel_code_segment());
    gdt.append(Descriptor::kernel_data_segment());
    gdt.load();

    let mut idt = InterruptDescriptorTable::new();
    unsafe {
        idt[IRQ_TIMER as usize].set_handler_addr(VirtAddr::from_ptr(timer_stub));
    }
    idt.load();

    pic_remap();
    pit_init();

    let g = &mut GRAPH;
    g.add_node(0, 100, 0, &[1, 3]);
    g.add_node(1, 80, 10, &[0, 2]);
    g.add_node(2, 60, 5, &[1, 3]);
    g.add_node(3, 40, 20, &[2, 0]);
    g.add_node(4, 80, 0, &[0]);

    init_node_stacks(g);
    serial_write("Process graph: 5 nodes, PIT preemption, yield_to_kernel\r\n");

    asm!("sti");

    while TICK_COUNT < 20 {
        asm!("hlt");
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
