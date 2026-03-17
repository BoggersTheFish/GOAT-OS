//! In-RAM filesystem (nodes as files/directories)

use alloc::string::ToString;
use alloc::string::String;
use alloc::vec::Vec;

const MAX_NAME: usize = 32;
const MAX_NODES: usize = 64;
const MAX_CHILDREN: usize = 16;
const MAX_FILE_SIZE: usize = 256;

#[derive(Clone, Copy)]
pub enum FsNodeKind {
    None,
    File,
    Dir,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct FsNode {
    pub kind: FsNodeKind,
    pub name: [u8; MAX_NAME],
    pub parent: usize,
    pub children: [usize; MAX_CHILDREN],
    pub data: [u8; MAX_FILE_SIZE],
    pub data_len: usize,
}

impl FsNode {
    const fn empty() -> Self {
        Self {
            kind: FsNodeKind::None,
            name: [0; MAX_NAME],
            parent: 0,
            children: [usize::MAX; MAX_CHILDREN],
            data: [0; MAX_FILE_SIZE],
            data_len: 0,
        }
    }

    fn name_str(&self) -> &str {
        let mut len = 0;
        while len < MAX_NAME && self.name[len] != 0 {
            len += 1;
        }
        core::str::from_utf8(&self.name[..len]).unwrap_or("?")
    }
}

static mut NODES: [FsNode; MAX_NODES] = [const { FsNode::empty() }; MAX_NODES];
static mut NODE_COUNT: usize = 0;

fn name_to_arr(s: &str) -> [u8; MAX_NAME] {
    let mut arr = [0u8; MAX_NAME];
    for (i, b) in s.bytes().take(MAX_NAME - 1).enumerate() {
        arr[i] = b;
    }
    arr
}

fn alloc_node() -> Option<usize> {
    unsafe {
        if NODE_COUNT >= MAX_NODES {
            return None;
        }
        let idx = NODE_COUNT;
        NODE_COUNT += 1;
        NODES[idx] = FsNode::empty();
        Some(idx)
    }
}

pub fn init() {
    unsafe {
        NODE_COUNT = 0;
        let root = alloc_node().unwrap();
        NODES[root].kind = FsNodeKind::Dir;
        NODES[root].name = name_to_arr("/");
        NODES[root].parent = root;
    }
}

fn find_child(parent: usize, name: &str) -> Option<usize> {
    unsafe {
        for i in 0..MAX_CHILDREN {
            let idx = NODES[parent].children[i];
            if idx == usize::MAX {
                break;
            }
            if NODES[idx].name_str() == name {
                return Some(idx);
            }
        }
        None
    }
}

fn add_child(parent: usize, idx: usize) -> bool {
    unsafe {
        for i in 0..MAX_CHILDREN {
            if NODES[parent].children[i] == usize::MAX {
                NODES[parent].children[i] = idx;
                NODES[idx].parent = parent;
                return true;
            }
        }
        false
    }
}

pub fn mkdir(path: &str) -> bool {
    unsafe {
        let parts: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return true;
        }
        let mut cur = 0;
        for (i, part) in parts.iter().enumerate() {
            if let Some(idx) = find_child(cur, part) {
                if matches!(NODES[idx].kind, FsNodeKind::Dir) {
                    cur = idx;
                } else {
                    return false;
                }
            } else if i == parts.len() - 1 {
                let new_idx = match alloc_node() {
                    Some(x) => x,
                    None => return false,
                };
                NODES[new_idx].kind = FsNodeKind::Dir;
                NODES[new_idx].name = name_to_arr(part);
                if add_child(cur, new_idx) {
                    return true;
                }
                return false;
            } else {
                return false;
            }
        }
        true
    }
}

pub fn touch(path: &str) -> bool {
    unsafe {
        let parts: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return false;
        }
        let file_name = parts[parts.len() - 1];
        let mut cur = 0;
        for part in &parts[..parts.len() - 1] {
            if let Some(idx) = find_child(cur, part) {
                if matches!(NODES[idx].kind, FsNodeKind::Dir) {
                    cur = idx;
                } else {
                    return false;
                }
            } else {
                let sub_path = "/".to_string() + &parts[..parts.len() - 1].join("/");
                if !mkdir(&sub_path) {
                    return false;
                }
                cur = resolve(&sub_path).unwrap_or(0);
            }
        }
        if find_child(cur, file_name).is_some() {
            return true;
        }
        let new_idx = match alloc_node() {
            Some(x) => x,
            None => return false,
        };
        NODES[new_idx].kind = FsNodeKind::File;
        NODES[new_idx].name = name_to_arr(file_name);
        add_child(cur, new_idx)
    }
}

pub fn write_file(path: &str, data: &[u8]) -> bool {
    unsafe {
        if let Some(idx) = resolve(path) {
            if matches!(NODES[idx].kind, FsNodeKind::File) {
                let len = data.len().min(MAX_FILE_SIZE);
                NODES[idx].data[..len].copy_from_slice(&data[..len]);
                NODES[idx].data_len = len;
                return true;
            }
        }
        false
    }
}

pub fn read_file(path: &str) -> Option<Vec<u8>> {
    unsafe {
        if let Some(idx) = resolve(path) {
            if matches!(NODES[idx].kind, FsNodeKind::File) {
                let len = NODES[idx].data_len;
                return Some(NODES[idx].data[..len].to_vec());
            }
        }
        None
    }
}

fn resolve(path: &str) -> Option<usize> {
    let parts: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return Some(0);
    }
    let mut cur = 0;
    for part in &parts {
        cur = find_child(cur, part)?;
    }
    Some(cur)
}

pub fn list_dir(path: &str) -> Vec<String> {
    unsafe {
        let idx = if path.is_empty() || path == "/" {
            0
        } else if let Some(i) = resolve(path) {
            i
        } else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for i in 0..MAX_CHILDREN {
            let c = NODES[idx].children[i];
            if c == usize::MAX {
                break;
            }
            out.push(NODES[c].name_str().to_string());
        }
        out
    }
}

pub fn cat(path: &str) -> Option<String> {
    read_file(path).and_then(|d| String::from_utf8(d).ok())
}

pub fn serialize_to(buf: &mut [u8]) -> usize {
    unsafe {
        let hdr_size = 8;
        if buf.len() < hdr_size + NODE_COUNT * core::mem::size_of::<FsNode>() {
            return 0;
        }
        *(buf.as_mut_ptr() as *mut usize) = NODE_COUNT;
        let data = core::slice::from_raw_parts(
            NODES.as_ptr() as *const u8,
            NODE_COUNT * core::mem::size_of::<FsNode>(),
        );
        buf[hdr_size..hdr_size + data.len()].copy_from_slice(data);
        hdr_size + data.len()
    }
}

pub fn deserialize_from(buf: &[u8]) -> bool {
    unsafe {
        if buf.len() < 8 {
            return false;
        }
        let count = *(buf.as_ptr() as *const usize);
        if count > MAX_NODES || buf.len() < 8 + count * core::mem::size_of::<FsNode>() {
            return false;
        }
        NODE_COUNT = count;
        let data = core::slice::from_raw_parts(
            buf.as_ptr().add(8),
            count * core::mem::size_of::<FsNode>(),
        );
        core::ptr::copy_nonoverlapping(
            data.as_ptr(),
            NODES.as_mut_ptr() as *mut u8,
            data.len(),
        );
        true
    }
}
