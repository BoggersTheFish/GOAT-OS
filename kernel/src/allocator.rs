//! Simple bump heap allocator (no free)

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

const HEAP_SIZE: usize = 262144;

#[repr(align(4096))]
struct HeapBacking([u8; HEAP_SIZE]);

static mut HEAP_BACKING: HeapBacking = HeapBacking([0; HEAP_SIZE]);
static HEAP_BUMPS: AtomicUsize = AtomicUsize::new(0);

pub struct BumpAllocator;

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        let size = layout.size();
        let base = HEAP_BACKING.0.as_mut_ptr() as usize;
        let mut bump = HEAP_BUMPS.load(Ordering::SeqCst);
        loop {
            let addr = base + bump;
            let aligned = (addr + align - 1) & !(align - 1);
            let new_bump = aligned - base + size;
            if new_bump > HEAP_SIZE {
                return core::ptr::null_mut();
            }
            match HEAP_BUMPS.compare_exchange(bump, new_bump, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => return aligned as *mut u8,
                Err(b) => bump = b,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

pub fn init() {}
