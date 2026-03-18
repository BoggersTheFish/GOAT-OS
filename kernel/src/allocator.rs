//! Kernel heap allocator using linked_list_allocator.
//! Supports free; enables proper deallocation for long-running kernel.

use linked_list_allocator::LockedHeap;

const HEAP_SIZE: usize = 4 * 1024 * 1024; // 4 MiB

#[repr(align(4096))]
struct HeapBacking([u8; HEAP_SIZE]);

static mut HEAP_BACKING: HeapBacking = HeapBacking([0; HEAP_SIZE]);
static mut HEAP_INIT: bool = false;

#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();

/// Initialize the kernel heap. Must be called once at boot before any allocation.
pub fn init() {
    unsafe {
        if !HEAP_INIT {
            let heap_start = HEAP_BACKING.0.as_mut_ptr();
            HEAP.lock().init(heap_start, HEAP_SIZE);
            HEAP_INIT = true;
        }
    }
}
