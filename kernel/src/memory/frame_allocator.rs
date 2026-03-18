//! Bitmap physical frame allocator.
//!
//! Phase 1 goal: provide alloc/free of 4 KiB frames based on Limine memory map.
//! This is the foundation for demand paging and per-process address spaces.

use limine::memory_map;

const FRAME_SIZE: u64 = 4096;

#[derive(Clone, Copy)]
pub struct Frame {
    pub phys: u64,
}

pub struct BitmapFrameAllocator {
    base: u64,
    frames: u64,
    bitmap: &'static mut [u64],
}

impl BitmapFrameAllocator {
    /// Initialize from Limine memory map entries.
    ///
    /// Strategy:
    /// - pick the largest usable span
    /// - build a bitmap covering that span
    /// - mark all frames as used, then mark usable frames as free
    ///
    /// Note: bitmap storage is provided by the caller (static buffer).
    pub fn init(
        entries: &[&memory_map::Entry],
        bitmap_storage: &'static mut [u64],
    ) -> Option<Self> {
        // Find largest usable region
        let mut best_base = 0u64;
        let mut best_len = 0u64;
        for e in entries {
            if e.entry_type != memory_map::EntryType::USABLE {
                continue;
            }
            let base = e.base;
            let len = e.length;
            if len > best_len {
                best_base = base;
                best_len = len;
            }
        }
        if best_len < FRAME_SIZE {
            return None;
        }

        let frames = best_len / FRAME_SIZE;
        let bits = frames as usize;
        let words = (bits + 63) / 64;
        if bitmap_storage.len() < words {
            return None;
        }

        let bitmap = &mut bitmap_storage[..words];
        for w in bitmap.iter_mut() {
            *w = u64::MAX; // all used
        }

        // Mark frames inside usable entries as free, but only within chosen span
        for e in entries {
            if e.entry_type != memory_map::EntryType::USABLE {
                continue;
            }
            let start = e.base.max(best_base);
            let end = (e.base + e.length).min(best_base + best_len);
            if end <= start {
                continue;
            }
            let mut p = align_up(start, FRAME_SIZE);
            while p + FRAME_SIZE <= end {
                let idx = ((p - best_base) / FRAME_SIZE) as usize;
                Self::bit_clear(bitmap, idx);
                p += FRAME_SIZE;
            }
        }

        Some(Self {
            base: best_base,
            frames,
            bitmap,
        })
    }

    pub fn alloc_frame(&mut self) -> Option<Frame> {
        for (wi, &word) in self.bitmap.iter().enumerate() {
            if word != u64::MAX {
                // has a free (0) bit
                let inv = !word;
                let bit = inv.trailing_zeros() as usize;
                let idx = wi * 64 + bit;
                if idx as u64 >= self.frames {
                    return None;
                }
                Self::bit_set(self.bitmap, idx);
                let phys = self.base + idx as u64 * FRAME_SIZE;
                return Some(Frame { phys });
            }
        }
        None
    }

    pub fn dealloc_frame(&mut self, frame: Frame) {
        if frame.phys < self.base {
            return;
        }
        let idx = ((frame.phys - self.base) / FRAME_SIZE) as usize;
        if idx as u64 >= self.frames {
            return;
        }
        Self::bit_clear(self.bitmap, idx);
    }

    #[inline]
    fn bit_set(bitmap: &mut [u64], idx: usize) {
        let w = idx / 64;
        let b = idx % 64;
        bitmap[w] |= 1u64 << b;
    }

    #[inline]
    fn bit_clear(bitmap: &mut [u64], idx: usize) {
        let w = idx / 64;
        let b = idx % 64;
        bitmap[w] &= !(1u64 << b);
    }
}

#[inline]
fn align_up(x: u64, a: u64) -> u64 {
    (x + (a - 1)) & !(a - 1)
}

