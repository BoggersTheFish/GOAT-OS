//! Persistence: serialize process graph + filesystem to RAM and optionally disk

use crate::disk;

const PERSIST_MAGIC: u64 = 0x5453_4F53_434B_5054; // "TSOSCKPT"
const FS_BUF_SIZE: usize = 16384;
const DISK_CHECKPOINT_SECTOR: u32 = 0;

#[repr(C)]
struct CheckpointHeader {
    magic: u64,
    graph_count: u32,
    fs_len: u32,
}

extern "C" {
    static _persist_start: u8;
    static _persist_end: u8;
}

fn persist_region() -> *mut u8 {
    unsafe { &_persist_start as *const u8 as *mut u8 }
}

fn persist_size() -> usize {
    unsafe { &_persist_end as *const u8 as usize - &_persist_start as *const u8 as usize }
}

pub fn do_checkpoint(graph_data: *const u8, graph_len: usize, fs_data: *const u8, fs_len: usize) -> bool {
    let region = persist_region();
    let size = persist_size();
    if size < core::mem::size_of::<CheckpointHeader>() + graph_len + fs_len {
        return false;
    }
    unsafe {
        let hdr = region as *mut CheckpointHeader;
        (*hdr).magic = PERSIST_MAGIC;
        (*hdr).graph_count = graph_len as u32;
        (*hdr).fs_len = fs_len as u32;
        core::ptr::copy_nonoverlapping(graph_data, region.add(core::mem::size_of::<CheckpointHeader>()), graph_len);
        core::ptr::copy_nonoverlapping(fs_data, region.add(core::mem::size_of::<CheckpointHeader>() + graph_len), fs_len);
    }
    let total = core::mem::size_of::<CheckpointHeader>() + graph_len + fs_len;
    let mut sector_buf = [0u8; 512];
    for (i, chunk) in unsafe { core::slice::from_raw_parts(region, total) }.chunks(512).enumerate() {
        sector_buf[..chunk.len()].copy_from_slice(chunk);
        if disk::write_sector(DISK_CHECKPOINT_SECTOR + i as u32, &sector_buf) {
            sector_buf.fill(0);
        }
    }
    true
}

pub fn try_restore() -> bool {
    let region = persist_region();
    let mut sector_buf = [0u8; 512];
    if disk::read_sector(DISK_CHECKPOINT_SECTOR, &mut sector_buf) {
        let magic = u64::from_le_bytes(sector_buf[0..8].try_into().unwrap_or([0; 8]));
        if magic == PERSIST_MAGIC {
            let graph_len = u32::from_le_bytes(sector_buf[8..12].try_into().unwrap_or([0; 4])) as usize;
            let fs_len = u32::from_le_bytes(sector_buf[12..16].try_into().unwrap_or([0; 4])) as usize;
            let total = 16 + graph_len + fs_len;
            let mut off = 0;
            for i in 0..((total + 511) / 512) {
                if disk::read_sector(DISK_CHECKPOINT_SECTOR + i as u32, &mut sector_buf) {
                    let copy_len = (total - off).min(512);
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            sector_buf.as_ptr(),
                            region.add(off),
                            copy_len,
                        );
                    }
                    off += copy_len;
                }
            }
            return true;
        }
    }
    unsafe { (*(region as *const CheckpointHeader)).magic == PERSIST_MAGIC }
}

pub fn restore_graph(dst: *mut u8, max_len: usize) -> usize {
    let region = persist_region();
    unsafe {
        let hdr = region as *const CheckpointHeader;
        if (*hdr).magic != PERSIST_MAGIC {
            return 0;
        }
        let n = ((*hdr).graph_count as usize).min(max_len);
        core::ptr::copy_nonoverlapping(region.add(core::mem::size_of::<CheckpointHeader>()), dst, n);
        n
    }
}

pub fn restore_fs(dst: *mut u8, max_len: usize) -> usize {
    let region = persist_region();
    unsafe {
        let hdr = region as *const CheckpointHeader;
        if (*hdr).magic != PERSIST_MAGIC || (*hdr).fs_len == 0 {
            return 0;
        }
        let graph_len = (*hdr).graph_count as usize;
        let fs_len = (*hdr).fs_len as usize;
        let n = fs_len.min(max_len);
        core::ptr::copy_nonoverlapping(
            region.add(core::mem::size_of::<CheckpointHeader>() + graph_len),
            dst,
            n,
        );
        n
    }
}
