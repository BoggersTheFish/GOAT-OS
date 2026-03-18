//! ELF64 loader for user binaries.
//! Parses headers, loads PT_LOAD segments into address space via map_page.

use crate::paging;
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

/// Load ELF64 binary into address space. Maps PT_LOAD segments, returns entry point.
pub fn load_elf(data: &[u8], cr3: u64, hhdm: u64) -> Result<ElfLoadInfo, ElfError> {
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

    let base = if e_type == ET_DYN { 0 } else { 0 };
    let entry = e_entry + base;

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
        let p_filesz = u64::from_le_bytes(data[phoff + 32..phoff + 40].try_into().unwrap());
        let p_memsz = u64::from_le_bytes(data[phoff + 40..phoff + 48].try_into().unwrap());

        let file_data_end = (p_offset + p_filesz) as usize;
        if file_data_end > data.len() {
            return Err(ElfError::LoadFailed);
        }

        let writable = (p_flags & PF_W) != 0;

        let mut vaddr = p_vaddr;
        let mut offset = 0u64;
        while offset < p_memsz {
            let layout = core::alloc::Layout::from_size_align(PAGE_SIZE as usize, PAGE_SIZE as usize).map_err(|_| ElfError::LoadFailed)?;
            let ptr = unsafe { alloc::alloc::alloc(layout) };
            if ptr.is_null() {
                return Err(ElfError::LoadFailed);
            }
            unsafe { core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize) };
            let phys = (ptr as u64).saturating_sub(hhdm);

            if !paging::map_page(cr3, hhdm, vaddr, phys, writable, true) {
                return Err(ElfError::LoadFailed);
            }

            let copy_len = (PAGE_SIZE.min(p_memsz - offset)) as usize;
            let file_start = (p_offset + offset) as usize;
            if file_start < file_data_end {
                let len = copy_len.min(file_data_end - file_start);
                unsafe {
                    ptr::copy_nonoverlapping(
                        data[file_start..file_start + len].as_ptr(),
                        ptr,
                        len,
                    );
                }
            }

            vaddr += PAGE_SIZE;
            offset += PAGE_SIZE;
        }
    }

    let stack_top = 0x7FFF_FFFF_F000u64;

    Ok(ElfLoadInfo { entry, stack_top })
}
