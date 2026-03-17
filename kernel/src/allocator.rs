//! Linked-list heap allocator with free, coalescing, and defragmentation

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

const HEAP_SIZE: usize = 131072; // 128 KiB
const MIN_BLOCK: usize = 32;
const HEADER_SIZE: usize = 16;

#[repr(align(16))]
struct HeapBacking([u8; HEAP_SIZE]);

static mut HEAP: HeapBacking = HeapBacking([0; HEAP_SIZE]);
static LIST_HEAD: AtomicPtr<FreeBlock> = AtomicPtr::new(core::ptr::null_mut());
static FREE_BLOCK_COUNT: AtomicUsize = AtomicUsize::new(0);
static TOTAL_FREE: AtomicUsize = AtomicUsize::new(0);

#[repr(C)]
struct FreeBlock {
    size: usize,
    next: *mut FreeBlock,
}

fn block_end(blk: *mut FreeBlock) -> usize {
    blk as usize + core::mem::size_of::<FreeBlock>() + unsafe { (*blk).size }
}

unsafe fn init_heap() {
    let base = HEAP.0.as_mut_ptr() as *mut FreeBlock;
    (*base).size = HEAP_SIZE - core::mem::size_of::<FreeBlock>();
    (*base).next = core::ptr::null_mut();
    LIST_HEAD.store(base, Ordering::SeqCst);
    FREE_BLOCK_COUNT.store(1, Ordering::SeqCst);
    TOTAL_FREE.store((*base).size, Ordering::SeqCst);
}

unsafe fn unlink_block(prev: *mut FreeBlock, curr: *mut FreeBlock) {
    if prev.is_null() {
        LIST_HEAD.store((*curr).next, Ordering::SeqCst);
    } else {
        (*prev).next = (*curr).next;
    }
    FREE_BLOCK_COUNT.fetch_sub(1, Ordering::SeqCst);
    TOTAL_FREE.fetch_sub((*curr).size, Ordering::SeqCst);
}

unsafe fn coalesce_with_neighbors(
    blk: *mut FreeBlock,
    blk_size: usize,
) -> (*mut FreeBlock, usize) {
    let blk_addr = blk as usize;
    let blk_end = blk_addr + blk_size;
    let mut merged_start = blk_addr;
    let mut merged_size = blk_size - core::mem::size_of::<FreeBlock>();

    let mut prev = core::ptr::null_mut();
    let mut curr = LIST_HEAD.load(Ordering::SeqCst);
    let mut before: (*mut FreeBlock, *mut FreeBlock) = (core::ptr::null_mut(), core::ptr::null_mut());
    let mut after: (*mut FreeBlock, *mut FreeBlock) = (core::ptr::null_mut(), core::ptr::null_mut());

    while !curr.is_null() {
        let c_end = block_end(curr);
        let c_start = curr as usize;
        if c_end == blk_addr {
            before = (prev, curr);
        } else if c_start == blk_end {
            after = (prev, curr);
        }
        prev = curr;
        curr = (*curr).next;
    }

    if !after.1.is_null() {
        unlink_block(after.0, after.1);
        merged_size += core::mem::size_of::<FreeBlock>() + (*after.1).size;
    }
    if !before.1.is_null() {
        let prev_before = if before.0 == after.1 { after.0 } else { before.0 };
        unlink_block(prev_before, before.1);
        merged_start = before.1 as usize;
        merged_size += core::mem::size_of::<FreeBlock>() + (*before.1).size;
    }

    let merged_blk = merged_start as *mut FreeBlock;
    (*merged_blk).size = merged_size;
    (*merged_blk).next = core::ptr::null_mut();
    FREE_BLOCK_COUNT.fetch_add(1, Ordering::SeqCst);
    TOTAL_FREE.fetch_add(merged_size, Ordering::SeqCst);
    (merged_blk, merged_size + core::mem::size_of::<FreeBlock>())
}

fn fragmentation_ratio() -> f32 {
    let count = FREE_BLOCK_COUNT.load(Ordering::SeqCst) as f32;
    let total = TOTAL_FREE.load(Ordering::SeqCst) as f32;
    if total <= 0.0 {
        return 0.0;
    }
    let overhead = count * (core::mem::size_of::<FreeBlock>() + MIN_BLOCK) as f32;
    (overhead / total).min(1.0)
}

unsafe fn defrag_pass() {
    let mut blocks: [*mut FreeBlock; 64] = [core::ptr::null_mut(); 64];
    let mut n = 0;
    let mut curr = LIST_HEAD.load(Ordering::SeqCst);
    while !curr.is_null() && n < 64 {
        blocks[n] = curr;
        n += 1;
        curr = (*curr).next;
    }
    for i in 0..n {
        for j in (i + 1)..n {
            if blocks[i].is_null() || blocks[j].is_null() {
                continue;
            }
            let a = blocks[i];
            let b = blocks[j];
            let a_end = block_end(a);
            let b_end = block_end(b);
            if a_end == b as usize {
                (*a).size += core::mem::size_of::<FreeBlock>() + (*b).size;
                (*a).next = (*b).next;
                let mut prev = core::ptr::null_mut();
                let mut c = LIST_HEAD.load(Ordering::SeqCst);
                while !c.is_null() {
                    if c == b {
                        unlink_block(prev, c);
                        break;
                    }
                    prev = c;
                    c = (*c).next;
                }
                blocks[j] = core::ptr::null_mut();
            } else if b_end == a as usize {
                (*b).size += core::mem::size_of::<FreeBlock>() + (*a).size;
                (*b).next = (*a).next;
                let mut prev = core::ptr::null_mut();
                let mut c = LIST_HEAD.load(Ordering::SeqCst);
                while !c.is_null() {
                    if c == a {
                        unlink_block(prev, c);
                        break;
                    }
                    prev = c;
                    c = (*c).next;
                }
                blocks[i] = core::ptr::null_mut();
            }
        }
    }
}

pub struct LinkedListAllocator;

unsafe impl GlobalAlloc for LinkedListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if fragmentation_ratio() > 0.30 {
            defrag_pass();
        }
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
                    FREE_BLOCK_COUNT.fetch_add(1, Ordering::SeqCst);
                    TOTAL_FREE.fetch_sub(block_size, Ordering::SeqCst);
                } else {
                    if prev.is_null() {
                        LIST_HEAD.store(blk.next, Ordering::SeqCst);
                    } else {
                        (*prev).next = blk.next;
                    }
                    FREE_BLOCK_COUNT.fetch_sub(1, Ordering::SeqCst);
                    TOTAL_FREE.fetch_sub(total_avail, Ordering::SeqCst);
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
        (*blk).next = core::ptr::null_mut();

        let (merged_blk, _) = coalesce_with_neighbors(blk, block_size);
        (*merged_blk).next = LIST_HEAD.load(Ordering::SeqCst);
        LIST_HEAD.store(merged_blk, Ordering::SeqCst);
    }
}

pub fn init() {
    unsafe { init_heap() };
}
