//! Kernel heap allocator using linked_list_allocator.
//! Supports free; enables proper deallocation for long-running kernel.

use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Initialize the kernel heap at a fixed virtual base.
/// Pages backing this region are mapped on demand by the page fault handler.
pub unsafe fn init_at(heap_base: usize, heap_initial_size: usize) {
    ALLOCATOR.lock().init(heap_base as *mut u8, heap_initial_size);
}
