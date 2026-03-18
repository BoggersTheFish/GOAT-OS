//! ELF64 loader for user binaries.
//! Parses headers, installs PT_LOAD VMAs, and maps/copies file-backed pages using frames.

use crate::paging;
use crate::memory::address_space::{AddressSpace, Vma};
use crate::memory::layout;
extern crate alloc;
use alloc::vec::Vec;
use core::ptr;

const EI_MAG0: usize = 0;
const EI_MAG1: usize = 1;
const EI_MAG2: usize = 2;
const EI_MAG3: usize = 3;
const EI_CLASS: usize = 4;
const EI_DATA: usize = 5;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PAGE_SIZE: u64 = 4096;

#[derive(Debug)]
pub enum ElfError {
    InvalidMagic,
    Not64Bit,
    NotLittleEndian,
    NotExecutable,
    WrongArch,
    InvalidPhdr,
    LoadFailed,
}

pub struct ElfLoadInfo {
    pub entry: u64,
    pub stack_top: u64,
}

#[inline]
fn align_down(x: u64, a: u64) -> u64 {
    x & !(a - 1)
}

#[inline]
fn align_up(x: u64, a: u64) -> u64 {
    (x + (a - 1)) & !(a - 1)
}

/// Load ELF64 binary into an `AddressSpace`.
///
/// P2.5: This loader must not allocate user backing pages from the kernel heap.
/// Instead it:
/// - installs a VMA for each PT_LOAD segment (covers full `p_memsz`)
/// - maps only the file-backed portion (`p_filesz`) by allocating physical frames
/// - copies bytes into frames via HHDM
/// - leaves the remainder demand-paged by the #PF handler
pub fn load_elf(aspace: &mut AddressSpace, data: &[u8]) -> Result<ElfLoadInfo, ElfError> {
    let hhdm = paging::hhdm_offset();
    if data.len() < 64 {
        return Err(ElfError::InvalidMagic);
    }
    if data[EI_MAG0] != 0x7f || data[EI_MAG1] != b'E' || data[EI_MAG2] != b'L' || data[EI_MAG3] != b'F' {
        return Err(ElfError::InvalidMagic);
    }
    if data[EI_CLASS] != ELFCLASS64 {
        return Err(ElfError::Not64Bit);
    }
    if data[EI_DATA] != ELFDATA2LSB {
        return Err(ElfError::NotLittleEndian);
    }

    let e_type = u16::from_le_bytes([data[16], data[17]]);
    if e_type != ET_EXEC && e_type != ET_DYN {
        return Err(ElfError::NotExecutable);
    }

    let e_machine = u16::from_le_bytes([data[18], data[19]]);
    if e_machine != EM_X86_64 {
        return Err(ElfError::WrongArch);
    }

    let e_entry = u64::from_le_bytes(data[24..32].try_into().unwrap());
    let e_phoff = u64::from_le_bytes(data[32..40].try_into().unwrap());
    let e_phnum = u16::from_le_bytes([data[56], data[57]]);
    let e_phentsize = u16::from_le_bytes([data[54], data[55]]);

    if e_phentsize != 56 {
        return Err(ElfError::InvalidPhdr);
    }

    // For ET_EXEC, respect the preferred load address (p_vaddr as-is).
    // For ET_DYN, we currently do not relocate (base=0), but keep the hook for future ASLR.
    let base = 0u64;
    let entry = e_entry + base;

    // Track mappings so we can roll back on partial failure (avoid leaking frames).
    let mut mapped: Vec<(u64, u64)> = Vec::new(); // (virt_page, phys_frame)

    for i in 0..e_phnum {
        let phoff = e_phoff as usize + i as usize * 56;
        if phoff + 56 > data.len() {
            continue;
        }
        let p_type = u32::from_le_bytes(data[phoff..phoff + 4].try_into().unwrap());
        if p_type != PT_LOAD {
            continue;
        }
        let p_flags = u32::from_le_bytes(data[phoff + 4..phoff + 8].try_into().unwrap());
        let p_offset = u64::from_le_bytes(data[phoff + 8..phoff + 16].try_into().unwrap());
        let p_vaddr = u64::from_le_bytes(data[phoff + 16..phoff + 24].try_into().unwrap()) + base;
        let p_align = u64::from_le_bytes(data[phoff + 48..phoff + 56].try_into().unwrap());
        let p_filesz = u64::from_le_bytes(data[phoff + 32..phoff + 40].try_into().unwrap());
        let p_memsz = u64::from_le_bytes(data[phoff + 40..phoff + 48].try_into().unwrap());

        if p_memsz < p_filesz {
            return Err(ElfError::LoadFailed);
        }

        let file_data_end = (p_offset + p_filesz) as usize;
        if file_data_end > data.len() {
            return Err(ElfError::LoadFailed);
        }

        let writable = (p_flags & PF_W) != 0;
        let executable = (p_flags & PF_X) != 0;

        let align = core::cmp::max(PAGE_SIZE, if p_align == 0 { PAGE_SIZE } else { p_align });

        // Install a VMA that covers the whole segment memory size (demand paging will fill gaps).
        let seg_start = align_down(p_vaddr, align);
        let seg_end = align_up(p_vaddr.saturating_add(p_memsz), align);
        if seg_end > seg_start {
            let vma = Vma::new(seg_start, seg_end - seg_start, writable, executable);
            if !vma.is_user_sane() {
                // rollback
                for (va, phys) in mapped.drain(..) {
                    let _ = paging::unmap_page(aspace.cr3, hhdm, va);
                    paging::dealloc_frame_phys(phys);
                }
                return Err(ElfError::LoadFailed);
            }
            // Reject overlapping VMAs.
            for existing in &aspace.vmas {
                let a0 = existing.start;
                let a1 = existing.end();
                let b0 = vma.start;
                let b1 = vma.end();
                if a0 < b1 && b0 < a1 {
                    for (va, phys) in mapped.drain(..) {
                        let _ = paging::unmap_page(aspace.cr3, hhdm, va);
                        paging::dealloc_frame_phys(phys);
                    }
                    return Err(ElfError::LoadFailed);
                }
            }
            aspace.vmas.push(vma);
        }

        // Map+copy only the file-backed portion.
        if p_filesz == 0 {
            continue;
        }

        let file_va_start = p_vaddr;
        let file_va_end = p_vaddr.saturating_add(p_filesz);
        let map_start = align_down(file_va_start, align);
        let map_end = align_up(file_va_end, align);

        let mut page = map_start;
        while page < map_end {
            // Allocate a fresh physical frame for this page.
            let phys = match paging::alloc_frame_phys() {
                Some(p) => p,
                None => {
                    for (va, phys) in mapped.drain(..) {
                        let _ = paging::unmap_page(aspace.cr3, hhdm, va);
                        paging::dealloc_frame_phys(phys);
                    }
                    return Err(ElfError::LoadFailed);
                }
            };
            unsafe { core::ptr::write_bytes((phys + hhdm) as *mut u8, 0, PAGE_SIZE as usize) };

            // Compute the byte range within this page that comes from the file.
            let page_data_start = core::cmp::max(file_va_start, page);
            let page_data_end = core::cmp::min(file_va_end, page + PAGE_SIZE);
            if page_data_end > page_data_start {
                let in_page_off = (page_data_start - page) as usize;
                let len = (page_data_end - page_data_start) as usize;
                let file_off = (p_offset + (page_data_start - p_vaddr)) as usize;
                if file_off + len > data.len() {
                    return Err(ElfError::LoadFailed);
                }
                unsafe {
                    ptr::copy_nonoverlapping(
                        data[file_off..file_off + len].as_ptr(),
                        ((phys + hhdm) as *mut u8).add(in_page_off),
                        len,
                    );
                }
            }

            // Map into the target process page tables (user=true, perms from segment flags).
            if !paging::map_page_ex(aspace.cr3, hhdm, page, phys, writable, true, executable) {
                paging::dealloc_frame_phys(phys);
                for (va, phys) in mapped.drain(..) {
                    let _ = paging::unmap_page(aspace.cr3, hhdm, va);
                    paging::dealloc_frame_phys(phys);
                }
                return Err(ElfError::LoadFailed);
            }
            mapped.push((page, phys));

            page += align;
        }
    }

    let stack_top = layout::USER_STACK_TOP;

    Ok(ElfLoadInfo { entry, stack_top })
}
