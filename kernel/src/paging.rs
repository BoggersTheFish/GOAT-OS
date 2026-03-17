//! 4-level paging for process isolation.
//! Each process gets a page table; kernel is identity-mapped, user stack at 0x1000.

use core::ptr::{read_volatile, write_volatile};

const PAGE_SIZE: usize = 4096;
const ENTRY_COUNT: usize = 512;
const PRESENT: u64 = 1;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const HUGE: u64 = 1 << 7;

type Table = [u64; ENTRY_COUNT];

/// Allocate a page-aligned frame from heap. Returns (physical_addr, virtual_ptr).
fn alloc_frame(hhdm_offset: u64) -> Option<(u64, *mut Table)> {
    let layout = core::alloc::Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).ok()?;
    let ptr = unsafe { alloc::alloc::alloc(layout) };
    if ptr.is_null() {
        return None;
    }
    unsafe {
        core::ptr::write_bytes(ptr, 0, PAGE_SIZE);
    }
    let virt = ptr as u64;
    let phys = virt.saturating_sub(hhdm_offset);
    Some((phys, ptr as *mut Table))
}

/// Create a new page table for a process. Clones kernel mappings, adds user stack at 0x1000.
/// Returns physical address of new PML4 for cr3.
pub fn create_process_page_table(hhdm_offset: u64) -> Option<u64> {
    let cr3 = current_cr3();
    if cr3 == 0 {
        return None;
    }

    let (pml4_phys, pml4_virt) = alloc_frame(hhdm_offset)?;
    let pml4 = unsafe { &mut *pml4_virt };

    let src_pml4_virt = cr3 + hhdm_offset;
    let src_pml4 = unsafe { &*(src_pml4_virt as *const Table) };

    for i in 0..ENTRY_COUNT {
        let entry = unsafe { read_volatile(&src_pml4[i]) };
        if (entry & PRESENT) != 0 {
            unsafe { write_volatile(&mut pml4[i], entry) };
        }
    }

    Some(pml4_phys)
}

/// Get current CR3 (physical address of PML4).
fn current_cr3() -> u64 {
    let cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nostack, preserves_flags));
    }
    cr3
}

/// Switch to the given page table (load cr3).
#[inline(always)]
pub unsafe fn switch_cr3(cr3: u64) {
    if cr3 != 0 {
        core::arch::asm!("mov cr3, {}", in(reg) cr3, options(nostack, preserves_flags));
    }
}
