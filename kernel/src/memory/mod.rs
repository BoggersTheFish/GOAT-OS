//! Memory subsystem primitives (Phase 1).
//!
//! All memory resources are treated as secondary nodes conceptually:
//! - Activation: recent access frequency / working-set membership
//! - Tension: fault pressure, fragmentation, or scarcity signals

pub mod frame_allocator;
pub mod layout;
pub mod address_space;

/// Initialize the kernel heap (LockedHeap) at the fixed kernel heap virtual base.
/// Must be called after the IDT has a page fault handler installed, since the
/// heap pages are demand-mapped on first access.
pub fn init_kernel_heap() {
    let base = layout::KERNEL_HEAP_BASE as usize;
    let size = layout::KERNEL_HEAP_INITIAL_SIZE as usize;
    unsafe {
        crate::allocator::init_at(base, size);
    }
}

