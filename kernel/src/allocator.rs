//! Linked-list heap allocator with free support

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicPtr, Ordering};

const HEAP_SIZE: usize = 131072; // 128 KiB
const MIN_BLOCK: usize = 32;
const HEADER_SIZE: usize = 16; // block_start + block_size before returned ptr

#[repr(align(16))]
struct HeapBacking([u8; HEAP_SIZE]);

static mut HEAP: HeapBacking = HeapBacking([0; HEAP_SIZE]);
static LIST_HEAD: AtomicPtr<FreeBlock> = AtomicPtr::new(core::ptr::null_mut());

#[repr(C)]
struct FreeBlock {
    size: usize,
    next: *mut FreeBlock,
}

unsafe fn init_heap() {
    let base = HEAP.0.as_mut_ptr() as *mut FreeBlock;
    (*base).size = HEAP_SIZE - core::mem::size_of::<FreeBlock>();
    (*base).next = core::ptr::null_mut();
    LIST_HEAD.store(base, Ordering::SeqCst);
}

pub struct LinkedListAllocator;

unsafe impl GlobalAlloc for LinkedListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(8).min(16);
        let block_size = (HEADER_SIZE + layout.size() + 15) & !15;

        let mut prev = core::ptr::null_mut();
        let mut curr = LIST_HEAD.load(Ordering::SeqCst);

        while !curr.is_null() {
            let blk = &*curr;
            let total_avail = core::mem::size_of::<FreeBlock>() + blk.size;
            if total_avail >= block_size {
                let remainder = total_avail - block_size;
                if remainder >= core::mem::size_of::<FreeBlock>() + MIN_BLOCK {
                    let split = (curr as *mut u8).add(block_size) as *mut FreeBlock;
                    (*split).size = remainder - core::mem::size_of::<FreeBlock>();
                    (*split).next = blk.next;
                    if prev.is_null() {
                        LIST_HEAD.store(split, Ordering::SeqCst);
                    } else {
                        (*prev).next = split;
                    }
                } else {
                    if prev.is_null() {
                        LIST_HEAD.store(blk.next, Ordering::SeqCst);
                    } else {
                        (*prev).next = blk.next;
                    }
                }
                let block_start = curr as *mut u8;
                *(block_start as *mut usize) = block_start as usize;
                *(block_start.add(8) as *mut usize) = block_size;
                return block_start.add(HEADER_SIZE);
            }
            prev = curr;
            curr = blk.next;
        }
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let block_start = ptr.sub(HEADER_SIZE);
        let block_size = *(block_start.add(8) as *const usize);
        let blk = block_start as *mut FreeBlock;
        (*blk).size = block_size - core::mem::size_of::<FreeBlock>();
        (*blk).next = LIST_HEAD.load(Ordering::SeqCst);
        LIST_HEAD.store(blk, Ordering::SeqCst);
    }
}

pub fn init() {
    unsafe { init_heap() };
}
