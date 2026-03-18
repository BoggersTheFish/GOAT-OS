//! Memory subsystem primitives (Phase 1).
//!
//! All memory resources are treated as secondary nodes conceptually:
//! - Activation: recent access frequency / working-set membership
//! - Tension: fault pressure, fragmentation, or scarcity signals

pub mod frame_allocator;

