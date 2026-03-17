//! PS/2 keyboard driver (ports 0x60, 0x64)

use core::sync::atomic::{AtomicU8, Ordering};

const DATA: u16 = 0x60;
const STATUS: u16 = 0x64;

static mut SCANCODE_BUF: [u8; 16] = [0; 16];
static mut BUF_HEAD: usize = 0;
static mut BUF_TAIL: usize = 0;
static BUF_COUNT: AtomicU8 = AtomicU8::new(0);

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let ret: u8;
    core::arch::asm!("in al, dx", in("dx") port, out("al") ret, options(nostack, preserves_flags));
    ret
}

fn scancode_to_ascii(sc: u8) -> Option<u8> {
    const TABLE: [u8; 128] = [
        0, 0, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', b'-', b'=', 0, 0,
        b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p', b'[', b']', b'\n', 0,
        b'a', b's', b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';', b'\'', b'`', 0, b'\\',
        b'z', b'x', b'c', b'v', b'b', b'n', b'm', b',', b'.', b'/', 0, b'*', 0, b' ',
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    if (sc as usize) < TABLE.len() {
        let c = TABLE[sc as usize];
        if c != 0 {
            return Some(c);
        }
    }
    None
}

pub fn poll() -> Option<u8> {
    unsafe {
        if (inb(STATUS) & 1) == 0 {
            return None;
        }
        let sc = inb(DATA);
        if (sc & 0x80) != 0 {
            return None;
        }
        scancode_to_ascii(sc)
    }
}

pub fn read_byte() -> Option<u8> {
    unsafe {
        if BUF_COUNT.load(Ordering::SeqCst) == 0 {
            if let Some(c) = poll() {
                SCANCODE_BUF[BUF_TAIL] = c;
                BUF_TAIL = (BUF_TAIL + 1) % 16;
                BUF_COUNT.fetch_add(1, Ordering::SeqCst);
            }
        }
        if BUF_COUNT.load(Ordering::SeqCst) == 0 {
            return None;
        }
        let c = SCANCODE_BUF[BUF_HEAD];
        BUF_HEAD = (BUF_HEAD + 1) % 16;
        BUF_COUNT.fetch_sub(1, Ordering::SeqCst);
        Some(c)
    }
}
