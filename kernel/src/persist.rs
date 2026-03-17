//! Persistence: serialize process graph + filesystem to reserved memory region

const PERSIST_MAGIC: u64 = 0x5453_4F53_434B_5054; // "TSOSCKPT"
const FS_BUF_SIZE: usize = 16384;

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
    true
}

pub fn try_restore() -> bool {
    let region = persist_region();
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
