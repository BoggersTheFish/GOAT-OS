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
const NX: u64 = 1u64 << 63;

type Table = [u64; ENTRY_COUNT];

/// Cached HHDM offset (set at boot).
static mut HHDM_OFFSET: u64 = 0;

/// Set the HHDM offset (boot-time).
pub unsafe fn set_hhdm_offset(offset: u64) {
    HHDM_OFFSET = offset;
}

/// Get the HHDM offset.
pub fn hhdm_offset() -> u64 {
    unsafe { HHDM_OFFSET }
}

/// Global physical frame allocator (initialized at boot).
static mut FRAME_ALLOC: Option<BitmapFrameAllocator> = None;

/// Provide the allocator instance after boot init.
pub unsafe fn init_frame_allocator(alloc: BitmapFrameAllocator) {
    FRAME_ALLOC = Some(alloc);
}

fn alloc_phys_frame() -> Option<Frame> {
    unsafe { FRAME_ALLOC.as_mut()?.alloc_frame() }
}

/// Allocate a physical 4KiB frame and return its physical address.
/// Safe to call from page fault handler (does not use heap).
pub fn alloc_frame_phys() -> Option<u64> {
    Some(alloc_phys_frame()?.phys)
}

/// Deallocate a physical 4KiB frame previously returned by `alloc_frame_phys`.
pub fn dealloc_frame_phys(phys: u64) {
    unsafe {
        if let Some(a) = FRAME_ALLOC.as_mut() {
            a.dealloc_frame(Frame { phys });
        }
    }
}

/// Map a physical frame at virtual address in the given page table.
pub fn map_page(cr3: u64, hhdm: u64, virt: u64, phys: u64, writable: bool, user: bool) -> bool {
    map_page_ex(cr3, hhdm, virt, phys, writable, user, true)
}

/// Map a physical frame with explicit executable flag.
pub fn map_page_ex(cr3: u64, hhdm: u64, virt: u64, phys: u64, writable: bool, user: bool, executable: bool) -> bool {
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
        let flags = PRESENT
            | (if writable { WRITABLE } else { 0 })
            | (if user { USER } else { 0 })
            | (if executable { 0 } else { NX });
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

/// Check whether a 4KiB page is currently mapped (PTE present).
pub fn is_page_present(cr3: u64, hhdm: u64, virt: u64) -> bool {
    let pml4_virt = (cr3 + hhdm) as *const Table;
    let p4_idx = ((virt >> 39) & 0x1FF) as usize;
    let p3_idx = ((virt >> 30) & 0x1FF) as usize;
    let p2_idx = ((virt >> 21) & 0x1FF) as usize;
    let p1_idx = ((virt >> 12) & 0x1FF) as usize;

    unsafe {
        let pml4 = &*pml4_virt;
        if pml4[p4_idx] & PRESENT == 0 {
            return false;
        }
        let p3 = &*(((pml4[p4_idx] & !0xFFF) + hhdm) as *const Table);
        if p3[p3_idx] & PRESENT == 0 {
            return false;
        }
        let p2 = &*(((p3[p3_idx] & !0xFFF) + hhdm) as *const Table);
        if p2[p2_idx] & PRESENT == 0 {
            return false;
        }
        if p2[p2_idx] & HUGE != 0 {
            // Mapped as huge page; treat as present.
            return true;
        }
        let p1 = &*(((p2[p2_idx] & !0xFFF) + hhdm) as *const Table);
        (p1[p1_idx] & PRESENT) != 0
    }
}

/// Set or clear the writable bit for an already-mapped 4KiB page.
/// Returns false if the page tables are missing or the PTE isn't present.
pub fn set_page_writable(cr3: u64, hhdm: u64, virt: u64, writable: bool) -> bool {
    let pml4_virt = (cr3 + hhdm) as *mut Table;
    let p4_idx = ((virt >> 39) & 0x1FF) as usize;
    let p3_idx = ((virt >> 30) & 0x1FF) as usize;
    let p2_idx = ((virt >> 21) & 0x1FF) as usize;
    let p1_idx = ((virt >> 12) & 0x1FF) as usize;

    unsafe {
        let pml4 = &mut *pml4_virt;
        if pml4[p4_idx] & PRESENT == 0 {
            return false;
        }
        let p3 = &mut *(((pml4[p4_idx] & !0xFFF) + hhdm) as *mut Table);
        if p3[p3_idx] & PRESENT == 0 {
            return false;
        }
        let p2 = &mut *(((p3[p3_idx] & !0xFFF) + hhdm) as *mut Table);
        if p2[p2_idx] & PRESENT == 0 {
            return false;
        }
        if p2[p2_idx] & HUGE != 0 {
            // Not supported yet (should be 4KiB pages for our kernel heap mapping).
            return false;
        }
        let p1 = &mut *(((p2[p2_idx] & !0xFFF) + hhdm) as *mut Table);
        if p1[p1_idx] & PRESENT == 0 {
            return false;
        }

        let mut entry = p1[p1_idx];
        if writable {
            entry |= WRITABLE;
        } else {
            entry &= !WRITABLE;
        }
        p1[p1_idx] = entry;
    }

    true
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
