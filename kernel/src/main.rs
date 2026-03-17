//! TS-OS Kernel — Strongest Node Framework
//!
//! Kernel = core_engine (structural search, pattern discovery, constraint resolution)
//! Scheduler = pure Strongest Node weighted spreading activation
//! PIT timer interrupt + preemptive scheduler + bump heap allocator

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicUsize, Ordering};

use limine::request::{RequestsEndMarker, RequestsStartMarker};
use limine::BaseRevision;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable};
use x86_64::structures::idt::InterruptDescriptorTable;
use x86_64::structures::tss::TaskStateSegment;
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
const HEAP_SIZE: usize = 65536;
const SYSCALL_VECTOR: u8 = 0x80;
const KERNEL_STACK_SIZE: usize = 4096;

static mut KERNEL_RSP: u64 = 0;
static mut CURRENT_NODE_IDX: usize = 0xFF;
static mut TICK_COUNT: u32 = 0;

extern "C" {
    fn timer_stub();
    fn syscall_stub();
}

#[repr(align(16))]
struct KernelStack([u8; KERNEL_STACK_SIZE]);
static mut KERNEL_STACK: KernelStack = KernelStack([0; KERNEL_STACK_SIZE]);
static mut TSS: MaybeUninit<TaskStateSegment> = MaybeUninit::uninit();

#[repr(align(4096))]
struct HeapBacking([u8; HEAP_SIZE]);

static mut HEAP_BACKING: HeapBacking = HeapBacking([0; HEAP_SIZE]);
static HEAP_BUMPS: AtomicUsize = AtomicUsize::new(0);

struct BumpAllocator;

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        let size = layout.size();
        let base = HEAP_BACKING.0.as_mut_ptr() as usize;
        let mut bump = HEAP_BUMPS.load(Ordering::SeqCst);
        loop {
            let addr = base + bump;
            let aligned = (addr + align - 1) & !(align - 1);
            let new_bump = aligned - base + size;
            if new_bump > HEAP_SIZE {
                return core::ptr::null_mut();
            }
            match HEAP_BUMPS.compare_exchange(bump, new_bump, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => return aligned as *mut u8,
                Err(b) => bump = b,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static HEAP: BumpAllocator = BumpAllocator;

#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    serial_write("ALLOC ERROR\r\n");
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

const SYS_WRITE: u64 = 1;
const SYS_YIELD: u64 = 2;
const SYS_SPAWN: u64 = 3;

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
                outb(COM1, *buf.add(i));
            }
            0
        }
        SYS_YIELD => {
            if CURRENT_NODE_IDX != 0xFF {
                GRAPH.nodes[CURRENT_NODE_IDX].saved_rsp = frame_ptr as u64;
            }
            let cur = if CURRENT_NODE_IDX != 0xFF {
                Some(CURRENT_NODE_IDX)
            } else {
                None
            };
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
        SYS_SPAWN => !0u64 as u64,
        _ => !0u64 as u64,
    }
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
    gdt.append(Descriptor::user_code_segment());
    gdt.append(Descriptor::user_data_segment());
    let tss = TSS.write(TaskStateSegment {
        privilege_stack_table: [VirtAddr::zero(); 3],
        interrupt_stack_table: [VirtAddr::zero(); 7],
        iomap_base: 0,
    });
    tss.privilege_stack_table[0] = VirtAddr::from_ptr(
        KERNEL_STACK.0.as_ptr().add(KERNEL_STACK_SIZE) as *const u8,
    );
    let tss_sel = gdt.append(Descriptor::tss_segment(unsafe { TSS.assume_init_ref() }));
    gdt.load();
    unsafe { x86_64::instructions::tables::load_tss(tss_sel) };

    let mut idt = InterruptDescriptorTable::new();
    unsafe {
        idt[IRQ_TIMER as usize].set_handler_addr(VirtAddr::from_ptr(timer_stub));
        idt[SYSCALL_VECTOR as usize].set_handler_addr(VirtAddr::from_ptr(syscall_stub));
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

    let buf = alloc::vec::Vec::from(b"Heap OK\r\n");
    for &b in &buf {
        outb(COM1, b);
    }

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
