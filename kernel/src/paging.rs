//! 4-level paging for process isolation.
//! Each process gets a page table; kernel is identity-mapped, user stack at 0x1000.

use crate::memory::frame_allocator::{BitmapFrameAllocator, Frame};
use core::ptr::{read_volatile, write_volatile};

const PAGE_SIZE: usize = 4096;
const ENTRY_COUNT: usize = 512;
const PRESENT: u64 = 1;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const HUGE: u64 = 1 << 7;

type Table = [u64; ENTRY_COUNT];

/// Global physical frame allocator (initialized at boot).
static mut FRAME_ALLOC: Option<BitmapFrameAllocator> = None;

/// Provide the allocator instance after boot init.
pub unsafe fn init_frame_allocator(alloc: BitmapFrameAllocator) {
    FRAME_ALLOC = Some(alloc);
}

fn alloc_phys_frame() -> Option<Frame> {
    unsafe { FRAME_ALLOC.as_mut()?.alloc_frame() }
}

/// Map a physical frame at virtual address in the given page table.
pub fn map_page(cr3: u64, hhdm: u64, virt: u64, phys: u64, writable: bool, user: bool) -> bool {
    let pml4_virt = (cr3 + hhdm) as *mut Table;
    let p4_idx = ((virt >> 39) & 0x1FF) as usize;
    let p3_idx = ((virt >> 30) & 0x1FF) as usize;
    let p2_idx = ((virt >> 21) & 0x1FF) as usize;
    let p1_idx = ((virt >> 12) & 0x1FF) as usize;

    unsafe {
        let pml4 = &mut *pml4_virt;
        if pml4[p4_idx] & PRESENT == 0 {
            let (p3_phys, p3_virt) = match alloc_frame(hhdm) {
                Some(x) => x,
                None => return false,
            };
            pml4[p4_idx] = p3_phys | PRESENT | WRITABLE | USER;
            core::ptr::write_bytes(p3_virt as *mut u8, 0, PAGE_SIZE);
        }
        let p3 = &mut *(((pml4[p4_idx] & !0xFFF) + hhdm) as *mut Table);
        if p3[p3_idx] & PRESENT == 0 {
            let (p2_phys, p2_virt) = match alloc_frame(hhdm) {
                Some(x) => x,
                None => return false,
            };
            p3[p3_idx] = p2_phys | PRESENT | WRITABLE | USER;
            core::ptr::write_bytes(p2_virt as *mut u8, 0, PAGE_SIZE);
        }
        let p2 = &mut *(((p3[p3_idx] & !0xFFF) + hhdm) as *mut Table);
        if p2[p2_idx] & PRESENT == 0 {
            let (p1_phys, p1_virt) = match alloc_frame(hhdm) {
                Some(x) => x,
                None => return false,
            };
            p2[p2_idx] = p1_phys | PRESENT | WRITABLE | USER;
            core::ptr::write_bytes(p1_virt as *mut u8, 0, PAGE_SIZE);
        }
        let p1 = &mut *(((p2[p2_idx] & !0xFFF) + hhdm) as *mut Table);
        let flags = PRESENT | (if writable { WRITABLE } else { 0 }) | (if user { USER } else { 0 });
        p1[p1_idx] = (phys & !0xFFF) | flags;
    }
    true
}

/// Unmap a page in the given page table. Returns the previous physical address if mapped.
pub fn unmap_page(cr3: u64, hhdm: u64, virt: u64) -> Option<u64> {
    let pml4_virt = (cr3 + hhdm) as *mut Table;
    let p4_idx = ((virt >> 39) & 0x1FF) as usize;
    let p3_idx = ((virt >> 30) & 0x1FF) as usize;
    let p2_idx = ((virt >> 21) & 0x1FF) as usize;
    let p1_idx = ((virt >> 12) & 0x1FF) as usize;

    unsafe {
        let pml4 = &mut *pml4_virt;
        if pml4[p4_idx] & PRESENT == 0 {
            return None;
        }
        let p3 = &mut *(((pml4[p4_idx] & !0xFFF) + hhdm) as *mut Table);
        if p3[p3_idx] & PRESENT == 0 {
            return None;
        }
        let p2 = &mut *(((p3[p3_idx] & !0xFFF) + hhdm) as *mut Table);
        if p2[p2_idx] & PRESENT == 0 {
            return None;
        }
        let p1 = &mut *(((p2[p2_idx] & !0xFFF) + hhdm) as *mut Table);
        if p1[p1_idx] & PRESENT == 0 {
            return None;
        }
        let prev = p1[p1_idx] & !0xFFF;
        p1[p1_idx] = 0;
        Some(prev)
    }
}

/// Allocate a page-aligned frame from heap. Returns (physical_addr, virtual_ptr).
fn alloc_frame(hhdm_offset: u64) -> Option<(u64, *mut Table)> {
    let frame = alloc_phys_frame()?;
    let phys = frame.phys;
    let virt = phys + hhdm_offset;
    let ptr = virt as *mut Table;
    unsafe { core::ptr::write_bytes(ptr as *mut u8, 0, PAGE_SIZE) };
    Some((phys, ptr))
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
