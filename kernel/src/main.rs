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

use scheduler::{NodeState, ProcessGraph, CWD_MAX, MAX_NODES, PARENT_NONE};
use process::Process;
use process::TrapFrame;
use memory::address_space::Vma;

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
use x86_64::registers::model_specific::{Efer, EferFlags};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::PrivilegeLevel;
use x86_64::VirtAddr;
use x86_64::instructions::interrupts;

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::fmt;
use core::fmt::Write;
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
// Current node index is owned by scheduler module now.
static mut TICK_COUNT: u32 = 0;
static mut HHDM_OFFSET: u64 = 0;
static mut KERNEL_CR3: u64 = 0;

// Frame allocator bitmap storage (u64 words). 262_144 words = 2 MiB bitmap => 16 GiB coverage.
#[repr(align(4096))]
struct FrameBitmap([u64; 262_144]);
static mut FRAME_BITMAP: FrameBitmap = FrameBitmap([0; 262_144]);

static mut GDT_STORE: MaybeUninit<GlobalDescriptorTable> = MaybeUninit::uninit();
static mut IDT_STORE: MaybeUninit<InterruptDescriptorTable> = MaybeUninit::uninit();

static NEEDS_RESCHEDULE: AtomicBool = AtomicBool::new(false);
static HEAP_PAGES_MAPPED: AtomicU64 = AtomicU64::new(0);

/// Very small, interrupt-safe serial logger lock.
static SERIAL_LOG_LOCK: AtomicBool = AtomicBool::new(false);
static SERIAL_READY: AtomicBool = AtomicBool::new(false);

struct SerialLogger;

impl SerialLogger {
    #[inline(always)]
    fn lock() {
        while SERIAL_LOG_LOCK
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    #[inline(always)]
    fn unlock() {
        SERIAL_LOG_LOCK.store(false, Ordering::Release);
    }
}

impl fmt::Write for SerialLogger {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        serial_write(s);
        Ok(())
    }
}

#[inline(always)]
fn ensure_serial_ready() {
    if SERIAL_READY.load(Ordering::Acquire) {
        return;
    }
    // Safe to call multiple times; first successful caller flips SERIAL_READY.
    serial_init();
    SERIAL_READY.store(true, Ordering::Release);
}

#[inline(always)]
pub(crate) fn log_args(args: fmt::Arguments) {
    // Must be safe from interrupt context: avoid deadlock by disabling interrupts while locked.
    interrupts::without_interrupts(|| {
        ensure_serial_ready();
        SerialLogger::lock();
        let _ = SerialLogger.write_fmt(args);
        SerialLogger::unlock();
    });
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        $crate::log_args(core::format_args!($($arg)*));
    }};
}

core::arch::global_asm!(
    r#"
    .section .text._start
    .global _start
_start:
    mov dx, 0x3F8
    mov al, 'A'
    out dx, al
    mov al, '\r'
    out dx, al
    mov al, '\n'
    out dx, al
    call kmain
    hlt
    jmp _start

    .text
    .global syscall_stub
    .global timer_stub
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

timer_stub:
    cli
    mov al, 0x20
    out 0x20, al
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
    sti
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
    fn timer_stub();
    fn double_fault_stub();
    fn bootstrap_switch(shell_rsp: u64, kernel_rsp_addr: *mut u64) -> !;
}

#[no_mangle]
pub unsafe extern "C" fn timer_handler(tf: &mut TrapFrame) -> u64 {
    interrupts::without_interrupts(|| {
        log!("timer_handler entered\r\n");
        TICK_COUNT = TICK_COUNT.wrapping_add(1);

        let cur = scheduler::current_idx();
        if NEEDS_RESCHEDULE.swap(false, Ordering::AcqRel) {
            scheduler::set_current_idx(None);
        }

        // Save current process register state (typed) with minimal work.
        if let Some(idx) = cur {
            let p = &mut graph().procs[idx];
            let mut ctx = process::ProcessContext::from_trap_frame(tf);
            // Store the current kernel trap-frame pointer in `context.rsp` so we can resume without
            // mutating legacy `saved_rsp` in the timer hot path.
            ctx.rsp = tf as *mut TrapFrame as u64;
            p.context = ctx;
            p.node.tension = p.node.tension.saturating_add(1);
        } else {
            // No current user process selected; keep kernel RSP so yield path remains safe.
            KERNEL_RSP = tf as *mut TrapFrame as u64;
        }

        // Minimal Strongest Node update to decide whether to switch.
        if let Some(c) = graph().select_strongest() {
            graph().decay_all(c);
            graph().spread_from(c);
        }
        let ns = match graph().select_strongest() {
            Some(i) => i,
            None => {
                unsafe { asm!("sti") };
                return 0;
            }
        };

        // If no actual change, keep running current process.
        if cur == Some(ns) {
            unsafe { asm!("sti") };
            return 0;
        }

        // We are switching processes now (do heavier work only on switch).
        log!("timer: about to schedule\r\n");
        let _old = cur;
        scheduler::set_current_idx(Some(ns));

        // Update IST only when we switch.
        let tss = &mut *TSS.as_mut_ptr();
        tss.interrupt_stack_table[0] = VirtAddr::new(graph().procs[ns].kernel_stack_top);

        // Switch address space.
        paging::switch_cr3(graph().procs[ns].aspace.cr3);
        log!("timer: switched to pid {}\r\n", graph().procs[ns].id);

        // Compute the incoming frame pointer:
        // - Prefer the process's last saved trap-frame pointer stored in `context.rsp` (set on preemption).
        // - Otherwise fall back to the creation-time `saved_rsp` (set during process init only).
        let mut next_rsp = graph().procs[ns].context.rsp;
        if next_rsp == 0 {
            next_rsp = graph().procs[ns].saved_rsp;
        }

        // Keep state updates light; avoid extra work in ISR.
        graph().procs[ns].state = NodeState::Running;
        let ret = next_rsp;
        unsafe { asm!("sti") };
        ret
    })
}

extern "x86-interrupt" fn page_fault_handler(_frame: InterruptStackFrame, _error: PageFaultErrorCode) {
    let addr = Cr2::read().as_u64();
    let err = _error.bits();

    // 1) Kernel heap faults: demand-map or upgrade permissions
    if memory::layout::in_kernel_heap_range(addr) {
        let page = addr & !0xFFFu64;
        let hhdm = unsafe { HHDM_OFFSET };

        if paging::is_page_present(unsafe { KERNEL_CR3 }, hhdm, page) {
            // Page exists but write faulted → force writable bit
            log!("PF: kernel-heap page present but not writable - forcing writable bit\r\n");
            if paging::set_page_writable(unsafe { KERNEL_CR3 }, hhdm, page, true) {
                log!("PF: kernel-heap successfully upgraded to writable\r\n");
                return;
            } else {
                log!("PF: kernel-heap failed to upgrade writable bit\r\n");
            }
        } else {
            log!("PF: kernel-heap mapping new page {:#x}...\r\n", page);
            if let Some(phys) = paging::alloc_frame_phys() {
                unsafe { core::ptr::write_bytes((phys + hhdm) as *mut u8, 0, 4096); }
                if paging::map_page_ex(unsafe { KERNEL_CR3 }, hhdm, page, phys, true, false, false) {
                    HEAP_PAGES_MAPPED.fetch_add(1, Ordering::Relaxed);
                    log!(
                        "PF: kernel-heap SUCCESS mapped page {:#x} (total: {})\r\n",
                        page,
                        HEAP_PAGES_MAPPED.load(Ordering::Relaxed)
                    );
                    return;
                }
            }
        }

        log!("PF: kernel-heap mapping/upgrade failed - halting\r\n");
        hcf();
    }

    let proc = match scheduler::current_process_mut() {
        Some(p) => p,
        None => {
            log!(
                "PF: no-current-proc cr2={:#x} err={:#x} (prune)\r\n",
                addr,
                err
            );
            prune_on_segv(addr);
            return;
        }
    };

    // 2) User faults: canonical + within usable user range + covered by a VMA in current AddressSpace.
    if memory::layout::is_canonical_user(addr) && memory::layout::in_user_usable_range(addr) {
        let cr3 = proc.aspace.cr3;
        if let Some(vma) = proc.aspace.covers_mut(addr) {
            log!(
                "PF: user cr2={:#x} err={:#x} pid={} cr3={:#x} vma=[{:#x}..{:#x}) w={} x={}\r\n",
                addr,
                err,
                proc.id,
                cr3,
                vma.start,
                vma.end(),
                vma.write,
                vma.exec
            );
            let page = addr & !0xFFFu64;
            let hhdm = unsafe { HHDM_OFFSET };
            if paging::is_page_present(cr3, hhdm, page) {
                return;
            }
            if let Some(phys) = paging::alloc_frame_phys() {
                unsafe { core::ptr::write_bytes((phys + hhdm) as *mut u8, 0, 4096) };
                let _ = paging::map_page_ex(cr3, hhdm, page, phys, vma.write, true, vma.exec);
                // Activation bump: VMA is \"hot\" when it faults/gets touched.
                vma.node.activation = vma.node.activation.saturating_add(4);
                return;
            }
            prune_on_segv(addr);
            return;
        }
    }

    // 3) Invalid access: prune/kill the current process node.
    log!(
        "PF: invalid cr2={:#x} err={:#x} pid={} cr3={:#x} canonical_user={} usable_user={} ctx.rip={:#x} ctx.rsp={:#x}\r\n",
        addr,
        err,
        proc.id,
        proc.aspace.cr3,
        memory::layout::is_canonical_user(addr),
        memory::layout::in_user_usable_range(addr),
        proc.context.rip,
        proc.context.rsp
    );
    prune_on_segv(addr);
}

fn prune_on_segv(addr: u64) {
    log!("SEGV: addr={:#x}\r\n", addr);
    console_write("Segmentation fault — node pruned due to invalid VA access at ");
    console_write_hex(addr);
    console_write("\r\n");

    if let Some(idx) = scheduler::current_idx() {
        if let Some(p) = scheduler::current_process_mut() {
            log!(
                "SEGV: pruning pid={} idx={} act={} tens={}\r\n",
                p.id,
                idx,
                p.node.activation,
                p.node.tension
            );
        }
        scheduler::prune_process(idx, 139); // 128+SIGSEGV
    }

    // Defer the actual reschedule to the timer interrupt.
    NEEDS_RESCHEDULE.store(true, Ordering::Release);
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
struct Ist0BootStack([u8; 4096]);
static mut IST0_BOOT_STACK: Ist0BootStack = Ist0BootStack([0; 4096]);


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
    SERIAL_READY.store(true, Ordering::Release);
}

#[inline(always)]
fn serial_write_byte(b: u8) {
    // Wait for transmitter holding register empty (LSR bit 5).
    unsafe {
        while (inb(LSR) & 0x20) == 0 {
            core::hint::spin_loop();
        }
        outb(COM1, b);
    }
}

pub(crate) fn serial_write(s: &str) {
    for b in s.bytes() {
        serial_write_byte(b);
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

fn console_write_hex(n: u64) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [0u8; 18];
    buf[0] = b'0';
    buf[1] = b'x';
    for i in 0..16 {
        let shift = 60 - (i * 4);
        let nibble = ((n >> shift) & 0xF) as usize;
        buf[2 + i] = HEX[nibble];
    }
    let s = unsafe { core::str::from_utf8_unchecked(&buf) };
    console_write(s);
}


fn graph() -> &'static mut ProcessGraph {
    scheduler::graph_mut()
}

fn map_process_kernel_stack(slot: usize) -> Option<(u64, u64)> {
    let guard = memory::layout::kernel_stack_slot_base(slot);
    let base = guard + memory::layout::KERNEL_STACK_GUARD_SIZE;
    let top = memory::layout::kernel_stack_top(slot);

    let hhdm = unsafe { HHDM_OFFSET };
    let cr3 = unsafe { KERNEL_CR3 };

    // Ensure guard page is unmapped.
    let _ = paging::unmap_page(cr3, hhdm, guard);

    let mut v = base;
    while v < top {
        if paging::is_page_present(cr3, hhdm, v) {
            v += 4096;
            continue;
        }
        let phys = paging::alloc_frame_phys()?;
        unsafe { core::ptr::write_bytes((phys + hhdm) as *mut u8, 0, 4096) };
        if !paging::map_page_ex(cr3, hhdm, v, phys, true, false, false) {
            return None;
        }
        v += 4096;
    }

    Some((base, top))
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
    let idx = match scheduler::current_idx() {
        Some(i) => i,
        None => {
            return;
        }
    };
    if idx == 0xFF {
        return;
    }
    let node = &mut graph().procs[idx];
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
        if !matches!(graph().procs[i].state, NodeState::Exited) {
            max_tension = max_tension.max(graph().procs[i].node.tension);
        }
    }
    if max_tension > 30 && graph().count() < MAX_NODES {
        let next_id = graph().count() as u32;
        let parent = scheduler::current_idx().unwrap_or(PARENT_NONE);
        if let Some(idx) = graph().try_add_node(next_id, 50, 0, &[], parent) {
            init_node_stacks_for(&mut graph().procs[idx]);
            // Default VMAs for new user node.
            let code = Vma::new(
                0x0000_0000_0040_0000,
                0x0000_0000_0080_0000 - 0x0000_0000_0040_0000,
                true,
                true,
            );
            let stack_top = 0x0000_7FFF_FFFF_F000u64;
            // Keep AddressSpace (owned by the process) in sync for #PF demand paging.
            graph().procs[idx].aspace.vmas.clear();
            graph().procs[idx].aspace.vmas.push(code);
            graph().procs[idx].aspace.vmas.push(Vma::new(
                stack_top - (8 * 1024 * 1024),
                8 * 1024 * 1024,
                true,
                false,
            ));
            graph().procs[idx].state = NodeState::Ready;
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
        graph().procs[ns].state = NodeState::Running;
        let do_switch = cur != Some(ns) || cur.is_none();
        if do_switch {
            scheduler::set_current_idx(Some(ns));
            unsafe { paging::switch_cr3(graph().procs[ns].aspace.cr3) };
            if graph().procs[ns].saved_rsp != 0 {
                return graph().procs[ns].saved_rsp;
            }
            // Each process has a mapped kernel stack; saved_rsp is always initialized at creation.
            return graph().procs[ns].saved_rsp;
        }
    } else {
        unsafe { paging::switch_cr3(KERNEL_CR3) };
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn syscall_handler(tf: &mut TrapFrame) -> u64 {
    let syscall = tf.syscall_number();
    match syscall {
        SYS_WRITE => {
            let _fd = tf.rdi;
            let buf = tf.rsi as *const u8;
            let len = tf.rdx as usize;
            if !validate_buf(buf, len) {
                tf.rax = !0u64;
                return 0;
            }
            for i in 0..len {
                let b = *buf.add(i);
                vga::write_byte(b);
                outb(COM1, b);
            }
            tf.rax = len as u64;
            0
        }
        SYS_READ => {
            let fd = tf.rdi;
            let buf = tf.rsi as *mut u8;
            let len = tf.rdx as usize;
            let mut n = 0usize;
            if fd == 0 && len > 0 && validate_buf(buf, len) {
                if let Some(c) = keyboard::read_byte().or_else(serial_read_byte) {
                    unsafe { *buf.add(0) = c };
                    n = 1;
                }
            }
            tf.rax = n as u64;
            0
        }
        SYS_YIELD => {
            if let Some(idx) = scheduler::current_idx() {
                graph().procs[idx].saved_rsp = tf as *mut TrapFrame as u64;
            }
            let cur = scheduler::current_idx();
            do_schedule(cur)
        }
        SYS_SPAWN => {
            let next_id = graph().count() as u32;
            let parent = scheduler::current_idx().unwrap_or(PARENT_NONE);
            if let Some(idx) = graph().try_add_node(next_id, 50, 0, &[], parent) {
                init_node_stacks_for(&mut graph().procs[idx]);
                // Default VMAs for new user node.
                let code = Vma::new(
                    0x0000_0000_0040_0000,
                    0x0000_0000_0080_0000 - 0x0000_0000_0040_0000,
                    true,
                    true,
                );
                let stack_top = 0x0000_7FFF_FFFF_F000u64;
                // Keep AddressSpace (owned by the process) in sync for #PF demand paging.
                graph().procs[idx].aspace.vmas.clear();
                graph().procs[idx].aspace.vmas.push(code);
                graph().procs[idx].aspace.vmas.push(Vma::new(
                    stack_top - (8 * 1024 * 1024),
                    8 * 1024 * 1024,
                    true,
                    false,
                ));
                graph().procs[idx].state = NodeState::Ready;
                tf.rax = idx as u64;
            } else {
                tf.rax = !0u64;
            }
            0
        }
        SYS_EXECVE => {
            // execve(path, path_len)
            log!("execve: start\r\n");
            let path_ptr = tf.rdi as *const u8;
            let path_len = tf.rsi as usize;
            if !validate_buf(path_ptr, path_len.min(64)) {
                log!("execve: failed validate_buf\r\n");
                tf.rax = !0u64;
                return 0;
            }
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = unsafe { *path_ptr.add(i) };
            }
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");
            log!("execve: reading file '{}'\r\n", path);
            let bytes = match fs::read_file(path) {
                Some(b) => b,
                None => {
                    log!("execve: failed read_file\r\n");
                    tf.rax = !0u64;
                    return 0;
                }
            };

            // 1. Create fresh AddressSpace + CR3
            let new_cr3 = match paging::create_process_page_table(HHDM_OFFSET) {
                Some(c) => c,
                None => {
                    log!("execve: failed create_process_page_table\r\n");
                    tf.rax = !0u64;
                    return 0;
                }
            };
            log!("execve: created new CR3 = {:#x}\r\n", new_cr3);
            let mut new_aspace = memory::address_space::AddressSpace::new(new_cr3);

            // 2. Load ELF (installs VMAs + maps file-backed pages)
            let info = match elf::load_elf(&mut new_aspace, &bytes) {
                Ok(i) => i,
                Err(_) => {
                    log!("execve: failed load_elf\r\n");
                    tf.rax = !0u64;
                    return 0;
                }
            };
            log!(
                "execve: loaded ELF entry = {:#x} vmas={}\r\n",
                info.entry,
                new_aspace.vmas.len()
            );

            // 3. Install user stack VMA (8 MiB, growing down)
            let stack_size: u64 = 8 * 1024 * 1024;
            let stack_start = memory::layout::USER_STACK_TOP - stack_size;
            let stack_vma = Vma::new(stack_start, stack_size, true, false);
            new_aspace.vmas.push(stack_vma);
            log!(
                "execve: installed stack VMA at {:#x} (size {:#x})\r\n",
                stack_start,
                stack_size
            );

            // 4. Eagerly map + zero the top stack page (optional but recommended)
            let hhdm = paging::hhdm_offset();
            let stack_top_page0 = memory::layout::USER_STACK_TOP - 4096;
            let stack_top_page1 = memory::layout::USER_STACK_TOP - 8192;
            for &va in &[stack_top_page1, stack_top_page0] {
                if let Some(phys) = paging::alloc_frame_phys() {
                    unsafe { core::ptr::write_bytes((phys + hhdm) as *mut u8, 0, 4096) };
                    let _ = paging::map_page_ex(new_aspace.cr3, hhdm, va, phys, true, true, false);
                    log!("execve: mapped stack page va={:#x} phys={:#x}\r\n", va, phys);
                } else {
                    log!("execve: warning: could not alloc stack page va={:#x}\r\n", va);
                }
            }

            // 5. Replace current process image
            let proc = match scheduler::current_process_mut() {
                Some(p) => p,
                None => {
                    log!("execve: failed current_process_mut\r\n");
                    tf.rax = !0u64;
                    return 0;
                }
            };
            let _old_cr3 = proc.aspace.cr3; // optional: reclaim later once we track user frames
            proc.aspace = new_aspace;
            log!(
                "execve: switching CR3 old={:#x} new={:#x}\r\n",
                _old_cr3,
                proc.aspace.cr3
            );
            unsafe { paging::switch_cr3(proc.aspace.cr3) };
            log!("execve: after switch_cr3\r\n");

            // 6. Reset context / TrapFrame for new binary
            proc.context = process::ProcessContext::zero();
            proc.context.rip = info.entry;
            proc.context.rflags = 0x202;

            // Our TrapFrame ABI only contains GPRs; the iret frame lives immediately after it
            // on the saved kernel stack: [15 regs][RIP, CS, RFLAGS, RSP, SS].
            let user_rsp = memory::layout::USER_STACK_TOP - 0x100;
            log!(
                "execve: patching iret frame rip={:#x} rsp={:#x}\r\n",
                info.entry,
                user_rsp
            );
            unsafe {
                // Clear live GPRs so the new image starts clean.
                core::ptr::write_bytes(tf as *mut TrapFrame as *mut u8, 0, core::mem::size_of::<TrapFrame>());
                // Safety net: explicitly clear common argument/scratch regs.
                tf.rax = 0;
                tf.rbx = 0;
                tf.rcx = 0;
                tf.rdx = 0;
                tf.rdi = 0;
                tf.rsi = 0;
                // Patch the iret frame that `iretq` will use when we return from the syscall.
                let iret = (tf as *mut TrapFrame as *mut u64).add(15);
                iret.add(0).write(info.entry); // RIP
                // CS at +1 left as-is (user selector already set up for user processes)
                iret.add(2).write(0x202u64); // RFLAGS
                iret.add(3).write(user_rsp); // RSP (user)
                // SS at +4 left as-is
            }
            proc.saved_rip = info.entry;
            tf.rax = 0;
            log!("execve: success\r\n");
            0
        }
        SYS_FORK => {
            // Minimal stub: not implemented yet (full address space clone later)
            tf.rax = !0u64;
            0
        }
        SYS_EXIT => {
            let status = tf.rdi as u8;
            if let Some(idx) = scheduler::current_idx() {
                graph().procs[idx].state = NodeState::Exited;
                graph().procs[idx].exit_status = status;
                graph().procs[idx].saved_rsp = tf as *mut TrapFrame as u64;
                let parent = graph().procs[idx].parent;
                if parent != PARENT_NONE && parent < graph().count() {
                    if matches!(graph().procs[parent].state, NodeState::Waiting) {
                        graph().procs[parent].state = NodeState::Ready;
                        let parent_rsp = graph().procs[parent].saved_rsp;
                        if parent_rsp != 0 {
                            unsafe {
                                *(parent_rsp as *mut u64) = (graph().procs[idx].id as u64) << 8 | status as u64;
                            }
                        }
                    }
                }
            }
            let cur = scheduler::current_idx();
            scheduler::set_current_idx(None);
            do_schedule(cur)
        }
        SYS_LS => {
            let path_ptr = tf.rdi as *const u8;
            let path_len = tf.rsi as usize;
            let out_ptr = tf.rdx as *mut u8;
            let out_len = tf.rcx as usize;
            if !validate_buf(path_ptr, path_len.min(64)) || !validate_buf(out_ptr, out_len) {
                tf.rax = !0u64;
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
            tf.rax = n as u64;
            0
        }
        SYS_CAT => {
            let path_ptr = tf.rdi as *const u8;
            let path_len = tf.rsi as usize;
            let out_ptr = tf.rdx as *mut u8;
            let out_len = tf.rcx as usize;
            if !validate_buf(path_ptr, path_len.min(64)) || !validate_buf(out_ptr, out_len) {
                tf.rax = !0u64;
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
            tf.rax = n as u64;
            0
        }
        SYS_PS => {
            let out_ptr = tf.rdi as *mut u8;
            let out_len = tf.rsi as usize;
            if !validate_buf(out_ptr, out_len) {
                tf.rax = !0u64;
                return 0;
            }
            let mut s = alloc::format!("pid  act  ten  state\r\n");
            for i in 0..graph().count() {
                let n = &graph().procs[i];
                let st = match n.state {
                    NodeState::Ready => "R",
                    NodeState::Running => "X",
                    NodeState::Exited => "E",
                    NodeState::Waiting => "W",
                };
                s += &alloc::format!(
                    "{}  {}  {}  {}\r\n",
                    n.id,
                    n.node.activation,
                    n.node.tension,
                    st
                );
            }
            let mut n = 0usize;
            for b in s.bytes() {
                if n < out_len {
                    unsafe { *out_ptr.add(n) = b };
                    n += 1;
                }
            }
            tf.rax = n as u64;
            0
        }
        SYS_TOUCH => {
            let path_ptr = tf.rdi as *const u8;
            let path_len = tf.rsi as usize;
            if !validate_buf(path_ptr, path_len.min(64)) {
                tf.rax = !0u64;
                return 0;
            }
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = unsafe { *path_ptr.add(i) };
            }
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");
            tf.rax = fs::touch(path) as u64;
            0
        }
        SYS_MKDIR => {
            let path_ptr = tf.rdi as *const u8;
            let path_len = tf.rsi as usize;
            if !validate_buf(path_ptr, path_len.min(64)) {
                tf.rax = !0u64;
                return 0;
            }
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = unsafe { *path_ptr.add(i) };
            }
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");
            tf.rax = fs::mkdir(path) as u64;
            0
        }
        SYS_WRITE_F => {
            let path_ptr = tf.rdi as *const u8;
            let path_len = tf.rsi as usize;
            let data_ptr = tf.rdx as *const u8;
            let data_len = tf.rcx as usize;
            if !validate_buf(path_ptr, path_len.min(64)) || !validate_buf(data_ptr, data_len.min(256)) {
                tf.rax = !0u64;
                return 0;
            }
            let mut path_buf = [0u8; 64];
            let plen = path_len.min(63);
            for i in 0..plen {
                path_buf[i] = *path_ptr.add(i);
            }
            let path = core::str::from_utf8(&path_buf[..plen]).unwrap_or("");
            let data = core::slice::from_raw_parts(data_ptr, data_len.min(256));
            tf.rax = fs::write_file(path, data) as u64;
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
            let path_ptr = tf.rdi as *const u8;
            let path_len = tf.rsi as usize;
            if !validate_buf(path_ptr, path_len.min(64)) {
                tf.rax = !0u64;
                return 0;
            }
            let mut path_buf = [0u8; 64];
            let len = path_len.min(63);
            for i in 0..len {
                path_buf[i] = unsafe { *path_ptr.add(i) };
            }
            let path = core::str::from_utf8(&path_buf[..len]).unwrap_or("");
            tf.rax = fs::rm(path) as u64;
            0
        }
        SYS_GETPID => {
            tf.rax = scheduler::current_idx()
                .map(|i| graph().procs[i].id as u64)
                .unwrap_or(!0u64);
            0
        }
        SYS_CHDIR => {
            let cur_idx = match scheduler::current_idx() {
                Some(i) => i,
                None => {
                    tf.rax = !0u64;
                    return 0;
                }
            };
            let path_ptr = tf.rdi as *const u8;
            let path_len = tf.rsi as usize;
            if !validate_buf(path_ptr, path_len.min(CWD_MAX)) {
                tf.rax = !0u64;
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
                let node = &mut graph().procs[cur_idx];
                let copy = path.len().min(CWD_MAX - 1);
                for i in 0..copy {
                    node.cwd[i] = path.as_bytes()[i];
                }
                node.cwd[copy] = 0;
                tf.rax = 1;
            } else {
                tf.rax = 0;
            }
            0
        }
        SYS_GETCWD => {
            let cur_idx = match scheduler::current_idx() {
                Some(i) => i,
                None => {
                    tf.rax = !0u64;
                    return 0;
                }
            };
            let out_ptr = tf.rdi as *mut u8;
            let out_len = tf.rsi as usize;
            if !validate_buf(out_ptr, out_len) {
                tf.rax = !0u64;
                return 0;
            }
            let node = &graph().procs[cur_idx];
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
            tf.rax = copy as u64;
            0
        }
        SYS_WAIT => {
            let cur_idx = match scheduler::current_idx() {
                Some(i) => i,
                None => {
                    tf.rax = !0u64;
                    return 0;
                }
            };
            let mut exited_child = None;
            for i in 0..graph().count() {
                if graph().procs[i].parent == cur_idx && matches!(graph().procs[i].state, NodeState::Exited) {
                    exited_child = Some(i);
                    break;
                }
            }
            if let Some(idx) = exited_child {
                let status = graph().procs[idx].exit_status;
                let pid = graph().procs[idx].id;
                graph().procs[idx].parent = PARENT_NONE;
                tf.rax = (pid as u64) << 8 | status as u64;
                0
            } else {
                graph().procs[cur_idx].state = NodeState::Waiting;
                graph().procs[cur_idx].saved_rsp = tf as *mut TrapFrame as u64;
                let cur = Some(cur_idx);
                do_schedule(cur)
            }
        }
        SYS_KILL => {
            let pid = tf.rdi as u32;
            let sig = tf.rsi as u32;
            if sig == 9 {
                for i in 0..graph().count() {
                    if graph().procs[i].id == pid {
                        graph().procs[i].state = NodeState::Exited;
                        graph().procs[i].exit_status = 128 + 9;
                        let parent = graph().procs[i].parent;
                        if parent != PARENT_NONE && parent < graph().count() {
                            if matches!(graph().procs[parent].state, NodeState::Waiting) {
                                graph().procs[parent].state = NodeState::Ready;
                                let parent_rsp = graph().procs[parent].saved_rsp;
                                if parent_rsp != 0 {
                                    unsafe {
                                        *(parent_rsp as *mut u64) = (pid as u64) << 8 | 137;
                                    }
                                }
                            }
                        }
                        tf.rax = 0;
                        return 0;
                    }
                }
            }
            tf.rax = !0u64;
            0
        }
        _ => !0u64,
    }
}

#[inline(never)]
unsafe extern "C" fn node_entry() {
    loop {
        let idx = match scheduler::current_idx() {
            Some(i) => i,
            None => break,
        };
        let node_id = graph().procs[idx].id;
        let act = graph().procs[idx].node.activation;
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
        let entry = if g.procs[i].id == 0 {
            shell::shell_main as *const () as u64
        } else {
            node_entry as *const () as u64
        };
        init_node_stacks_for_with_entry(&mut g.procs[i], entry);
    }
}

fn init_node_stacks_for(node: &mut Process) {
    let entry = node_entry as *const () as u64;
    init_node_stacks_for_with_entry(node, entry);
}

// TrapFrame (15 regs) + iret frame (5 qwords)
const INIT_FRAME_SIZE: usize = (15 + 5) * 8;

fn init_node_stacks_for_with_entry(node: &mut Process, entry: u64) {
    let (code_sel, data_sel) = if node.id == 0 {
        (0x08u64, 0x10u64)
    } else {
        (0x1bu64, 0x23u64)
    };
    let rflags = 0x202u64;
    unsafe {
        node.aspace.cr3 = if HHDM_OFFSET != 0 {
            paging::create_process_page_table(HHDM_OFFSET).unwrap_or(KERNEL_CR3)
        } else {
            KERNEL_CR3
        };
        let (kbase, ktop) = map_process_kernel_stack(node.id as usize).unwrap_or((0, 0));
        node.kernel_stack_base = kbase;
        node.kernel_stack_top = ktop;

        let frame_base = (ktop - (INIT_FRAME_SIZE as u64)) as *mut u64;
        let stack_rsp = frame_base as u64;
        // Zero the TrapFrame register save area (15 qwords).
        for j in 0..15 {
            frame_base.add(j).write(0);
        }
        // iretq frame immediately after TrapFrame
        frame_base.add(15).write(entry); // RIP
        frame_base.add(16).write(code_sel); // CS
        frame_base.add(17).write(rflags); // RFLAGS
        frame_base.add(18).write(stack_rsp); // RSP
        frame_base.add(19).write(data_sel); // SS

        // Initial resumable frame pointer points at TrapFrame (r15..rax).
        node.saved_rsp = stack_rsp;

        // Seed the canonical typed context from the initial TrapFrame + iret frame so a
        // never-before-run process has a valid `context`.
        let tf = &*(stack_rsp as *const TrapFrame);
        let mut ctx = process::ProcessContext::from_trap_frame(tf);
        // Save initial resume pointer + iret frame values.
        ctx.rsp = stack_rsp;
        ctx.rip = *frame_base.add(15);
        ctx.cs = *frame_base.add(16);
        ctx.rflags = *frame_base.add(17);
        ctx.ss = *frame_base.add(19);
        node.context = ctx;
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
    log!("K1 - Kernel entry\r\n");

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
        paging::set_hhdm_offset(HHDM_OFFSET);
    }
    unsafe {
        asm!("mov {}, cr3", out(reg) KERNEL_CR3, options(nostack, preserves_flags));
    }
    // Enable NX so we can map kernel heap pages as non-executable.
    unsafe {
        let flags = Efer::read() | EferFlags::NO_EXECUTE_ENABLE;
        Efer::write(flags);
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
    let ist0_top = VirtAddr::new(unsafe { &IST0_BOOT_STACK as *const _ as u64 } + 4096);
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
        GDT_STORE.write(gdt);
        (&*GDT_STORE.as_ptr()).load();
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
    // Use assembly stub so we can save/restore full TrapFrame regs.
    unsafe {
        idt[IRQ_TIMER as usize]
            .set_handler_addr(VirtAddr::new(timer_stub as *const () as u64));
    }
    unsafe {
        idt[SYSCALL_VECTOR as usize]
            .set_handler_addr(VirtAddr::new(syscall_stub as *const () as u64))
            .set_privilege_level(PrivilegeLevel::Ring3);
    }
    idt.page_fault.set_handler_fn(page_fault_handler);
    unsafe {
        IDT_STORE.write(idt);
        (&*IDT_STORE.as_ptr()).load();
    }

    // Initialize kernel heap AFTER IDT has page fault handler installed.
    memory::init_kernel_heap();
    log!(
        "heap: base={:#x} init_size={:#x} mapped_pages={} mapped_bytes={:#x}\r\n",
        memory::layout::KERNEL_HEAP_BASE,
        memory::layout::KERNEL_HEAP_INITIAL_SIZE,
        HEAP_PAGES_MAPPED.load(Ordering::Relaxed),
        HEAP_PAGES_MAPPED.load(Ordering::Relaxed) * 4096
    );

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
        scheduler::init_global_graph(ProcessGraph::new());
    }
    if graph().count() == 0 {
        graph().add_node(0, 100, 0, &[], PARENT_NONE);
    }
    // Boot status banner (serial): processes + strongest + CR3.
    let proc_count = graph().count();
    let strongest = graph().select_strongest();
    log!("boot: processes={}\r\n", proc_count);
    if let Some(si) = strongest {
        let p = &graph().procs[si];
        log!(
            "boot: strongest idx={} pid={} act={} tens={} cr3={:#x}\r\n",
            si,
            p.id,
            p.node.activation,
            p.node.tension,
            p.aspace.cr3
        );
    } else {
        log!("boot: strongest=<none>\r\n");
    }

    // Default VMAs for newly spawned user nodes (Phase 1 baseline):
    // keep legacy fixed array for now (scheduler still uses it), and keep AddressSpace (owned by process) in sync.
    for n in graph().procs.iter_mut() {
        if n.id == 0 {
            continue;
        }
        let stack_top = 0x0000_7FFF_FFFF_F000u64;
        n.aspace.vmas.clear();
        n.aspace.vmas.push(Vma::new(
            0x0000_0000_0040_0000,
            0x0000_0000_0080_0000 - 0x0000_0000_0040_0000,
            true,
            true,
        ));
        n.aspace.vmas.push(Vma::new(
            stack_top - (8 * 1024 * 1024),
            8 * 1024 * 1024,
            true,
            false,
        ));
    }
    init_node_stacks(graph());

    // Save kernel RSP and switch to shell.
    // bootstrap_switch expects RSP to point at the iret frame (RIP..SS),
    // which lives immediately after the 15-qword TrapFrame register area.
    let shell_base = graph().procs[0].saved_rsp;
    let shell_rsp = shell_base + (15 * 8) as u64;
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
    // Very early debug: ensure COM1 is initialized before any heap/paging/logging.
    serial_init();
    serial_write("K1 - Kernel entry\r\n");
    full_boot();
}

#[panic_handler]
fn rust_panic(_info: &core::panic::PanicInfo) -> ! {
    log!("PANIC\r\n");
    console_write("PANIC\r\n");
    hcf();
}

pub(crate) fn hcf() -> ! {
    if let Some(p) = scheduler::current_process_mut() {
        log!(
            "HCF: pid={} cr3={:#x} act={} tens={}\r\n",
            p.id,
            p.aspace.cr3,
            p.node.activation,
            p.node.tension
        );
    } else {
        log!("HCF: no current process\r\n");
    }
    loop {
        unsafe { asm!("hlt") };
    }
}
