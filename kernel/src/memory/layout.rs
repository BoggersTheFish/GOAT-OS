//! Canonical virtual address layout for TS-OS (Phase 1).
//!
//! This file defines the allowed user lower-half range, the null guard page,
//! and the kernel heap virtual region used for demand-mapped kernel heap.

pub const USER_NULL_GUARD_END: u64 = 0x1000;
pub const USER_LOWER_START: u64 = 0x1000;
pub const USER_LOWER_END: u64 = 0x0000_7FFF_FFFF_FFFF;

pub const USER_TEXT_BASE: u64 = 0x0000_0000_0040_0000;
pub const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_F000;

pub const KERNEL_HEAP_BASE: u64 = 0xFFFF_FFFF_8000_0000;
pub const KERNEL_HEAP_INITIAL_SIZE: u64 = 16 * 1024 * 1024;
pub const KERNEL_HEAP_MAX_SIZE: u64 = 512 * 1024 * 1024;

// Kernel per-process interrupt stacks (Phase 2.4).
//
// Each process gets one slot in this region:
//   [guard_page (unmapped)][stack_pages (mapped, NX, kernel-only)]
pub const KERNEL_STACK_REGION_BASE: u64 = 0xFFFF_FF80_0000_0000;
pub const KERNEL_STACK_SLOT_SIZE: u64 = 36 * 4096; // 32KiB stack + 4KiB guard
pub const KERNEL_STACK_SIZE: u64 = 32 * 1024; // 8 pages
pub const KERNEL_STACK_GUARD_SIZE: u64 = 4096;

#[inline]
pub fn kernel_stack_slot_base(slot: usize) -> u64 {
    KERNEL_STACK_REGION_BASE + (slot as u64) * KERNEL_STACK_SLOT_SIZE
}

#[inline]
pub fn kernel_stack_top(slot: usize) -> u64 {
    // Top of mapped stack pages (guard is below).
    kernel_stack_slot_base(slot) + KERNEL_STACK_GUARD_SIZE + KERNEL_STACK_SIZE
}

#[inline]
pub fn is_canonical_user(addr: u64) -> bool {
    (addr >> 48) == 0
}

#[inline]
pub fn in_user_usable_range(addr: u64) -> bool {
    addr >= USER_LOWER_START && addr <= USER_LOWER_END
}

#[inline]
pub fn in_kernel_heap_range(addr: u64) -> bool {
    addr >= KERNEL_HEAP_BASE && addr < KERNEL_HEAP_BASE + KERNEL_HEAP_MAX_SIZE
}

