//! x86_64 4-level paging: identity map kernel, user mappings, per-process page dirs

use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

const PAGE_SIZE: usize = 4096;
const USER_SPACE_SIZE: usize = 2 * 1024 * 1024; // 2 MiB user space
const PAGE_TABLE_ENTRIES: usize = 512;

fn align_up(addr: u64, align: u64) -> u64 {
    (addr + align - 1) & !(align - 1)
}

fn align_down(addr: u64, align: u64) -> u64 {
    addr & !(align - 1)
}

static mut MAPPER: Option<OffsetPageTable<'static>> = None;
static mut FRAME_ALLOC: Option<SimpleFrameAllocator> = None;
static mut HHDM_OFFSET: u64 = 0;

pub struct SimpleFrameAllocator {
    next: u64,
    end: u64,
}

impl SimpleFrameAllocator {
    pub fn new(start: u64, size: u64) -> Self {
        let start_frame = align_up(start, PAGE_SIZE as u64);
        let end_frame = align_down(start + size, PAGE_SIZE as u64);
        Self {
            next: start_frame,
            end: end_frame,
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for SimpleFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        if self.next >= self.end {
            return None;
        }
        let frame = PhysFrame::containing_address(PhysAddr::new(self.next));
        self.next += PAGE_SIZE as u64;
        Some(frame)
    }
}

pub fn init(physical_memory_offset: u64) {
    unsafe {
        HHDM_OFFSET = physical_memory_offset;
        let cr3 = x86_64::registers::control::Cr3::read();
        let virt = VirtAddr::new(cr3.0.start_address().as_u64() + physical_memory_offset);
        let table = core::slice::from_raw_parts_mut(virt.as_mut_ptr(), PAGE_TABLE_ENTRIES);
        let mapper = OffsetPageTable::new(
            PageTable::from_raw_ptr(table.as_mut_ptr()),
            VirtAddr::new(physical_memory_offset),
        );
        MAPPER = Some(mapper);
    }
}

pub fn init_frame_allocator(region_start: u64, region_len: u64) {
    FRAME_ALLOC = Some(SimpleFrameAllocator::new(region_start, region_len));
}

pub fn map_user_space() -> bool {
    unsafe {
        let mapper = MAPPER.as_mut().unwrap();
        let alloc = FRAME_ALLOC.as_mut().unwrap();
        let num_pages = USER_SPACE_SIZE / PAGE_SIZE;
        let flags =
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
        for i in 0..num_pages {
            let virt = VirtAddr::new((i * PAGE_SIZE) as u64);
            let page = Page::containing_address(virt);
            if mapper.translate_page(page).is_some() {
                continue;
            }
            let frame = match alloc.allocate_frame() {
                Some(f) => f,
                None => return false,
            };
            match mapper.map_to(page, frame, flags, alloc) {
                Ok(fl) => fl.flush(),
                Err(_) => return false,
            }
        }
        true
    }
}

pub fn mapper() -> Option<&'static mut OffsetPageTable<'static>> {
    unsafe { MAPPER.as_mut() }
}

pub fn frame_allocator() -> Option<&'static mut SimpleFrameAllocator> {
    unsafe { FRAME_ALLOC.as_mut() }
}

pub const PROCESS_STACK_TOP: u64 = 0x2000;

pub fn create_process_page_tables() -> Option<u64> {
    unsafe {
        let alloc = FRAME_ALLOC.as_mut()?;
        let pml4_frame = alloc.allocate_frame()?;
        let pml4_phys = pml4_frame.start_address().as_u64();
        let pml4_virt = VirtAddr::new(pml4_phys + HHDM_OFFSET);
        let pml4 = core::slice::from_raw_parts_mut(pml4_virt.as_mut_ptr::<u64>(), PAGE_TABLE_ENTRIES);
        core::ptr::write_bytes(pml4.as_mut_ptr(), 0, PAGE_TABLE_ENTRIES);

        let cur_cr3 = x86_64::registers::control::Cr3::read();
        let cur_pml4_virt = VirtAddr::new(cur_cr3.0.start_address().as_u64() + HHDM_OFFSET);
        let cur_pml4 = core::slice::from_raw_parts(cur_pml4_virt.as_ptr::<u64>(), PAGE_TABLE_ENTRIES);

        for i in 256..PAGE_TABLE_ENTRIES {
            pml4[i] = cur_pml4[i];
        }
        pml4[511] = pml4_phys | 0x03;

        let pdpt_frame = alloc.allocate_frame()?;
        let pdpt_phys = pdpt_frame.start_address().as_u64();
        pml4[0] = pdpt_phys | 0x03;
        let pdpt = core::slice::from_raw_parts_mut(
            (pdpt_phys + HHDM_OFFSET) as *mut u64,
            PAGE_TABLE_ENTRIES,
        );
        core::ptr::write_bytes(pdpt.as_mut_ptr(), 0, PAGE_TABLE_ENTRIES);

        let pd_frame = alloc.allocate_frame()?;
        let pd_phys = pd_frame.start_address().as_u64();
        pdpt[0] = pd_phys | 0x03;
        let pd = core::slice::from_raw_parts_mut((pd_phys + HHDM_OFFSET) as *mut u64, PAGE_TABLE_ENTRIES);
        core::ptr::write_bytes(pd.as_mut_ptr(), 0, PAGE_TABLE_ENTRIES);

        let pt_frame = alloc.allocate_frame()?;
        let pt_phys = pt_frame.start_address().as_u64();
        pd[0] = pt_phys | 0x03;
        let pt = core::slice::from_raw_parts_mut((pt_phys + HHDM_OFFSET) as *mut u64, PAGE_TABLE_ENTRIES);
        core::ptr::write_bytes(pt.as_mut_ptr(), 0, PAGE_TABLE_ENTRIES);

        let num_pages = USER_SPACE_SIZE / PAGE_SIZE;
        for i in 0..num_pages.min(PAGE_TABLE_ENTRIES) {
            let frame = alloc.allocate_frame()?;
            pt[i] = frame.start_address().as_u64() | 0x07;
        }

        let stack_frame = alloc.allocate_frame()?;
        pt[1] = stack_frame.start_address().as_u64() | 0x07;

        Some(pml4_phys)
    }
}

pub fn switch_cr3(cr3: u64) {
    use x86_64::registers::control::Cr3;
    unsafe {
        let (_, flags) = Cr3::read();
        Cr3::write(Cr3::new(
            PhysFrame::containing_address(PhysAddr::new(cr3)),
            flags,
        ));
    }
}
