//! PS/2 keyboard driver: shift/caps lock support, circular input buffer

use core::sync::atomic::{AtomicUsize, Ordering};

const DATA: u16 = 0x60;
const STATUS: u16 = 0x64;
const BUF_SIZE: usize = 128;

const SC_LSHIFT: u8 = 0x2A;
const SC_RSHIFT: u8 = 0x36;
const SC_CAPSLOCK: u8 = 0x3A;

static mut SHIFT: bool = false;
static mut CAPS: bool = false;

static mut RING_BUF: [u8; BUF_SIZE] = [0; BUF_SIZE];
static RING_HEAD: AtomicUsize = AtomicUsize::new(0);
static RING_TAIL: AtomicUsize = AtomicUsize::new(0);
static RING_COUNT: AtomicUsize = AtomicUsize::new(0);

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let ret: u8;
    core::arch::asm!("in al, dx", in("dx") port, out("al") ret, options(nostack, preserves_flags));
    ret
}

fn scancode_to_ascii(sc: u8, shifted: bool) -> Option<u8> {
    const LOWER: [u8; 58] = [
        0, 0, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', b'-', b'=', 0, 0,
        b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p', b'[', b']', b'\n', 0,
        b'a', b's', b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';', b'\'', b'`', 0, b'\\',
        b'z', b'x', b'c', b'v', b'b', b'n', b'm', b',', b'.', b'/', 0, b'*', 0, b' ',
    ];
    const UPPER: [u8; 58] = [
        0, 0, b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')', b'_', b'+', 0, 0,
        b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I', b'O', b'P', b'{', b'}', b'\n', 0,
        b'A', b'S', b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':', b'"', b'~', 0, b'|',
        b'Z', b'X', b'C', b'V', b'B', b'N', b'M', b'<', b'>', b'?', 0, b'*', 0, b' ',
    ];
    if (sc as usize) < LOWER.len() {
        let c = if shifted { UPPER[sc as usize] } else { LOWER[sc as usize] };
        if c != 0 {
            return Some(c);
        }
    }
    None
}

fn push_byte(b: u8) {
    if RING_COUNT.load(Ordering::SeqCst) >= BUF_SIZE {
        return;
    }
    unsafe {
        let tail = RING_TAIL.load(Ordering::SeqCst);
        RING_BUF[tail] = b;
        RING_TAIL.store((tail + 1) % BUF_SIZE, Ordering::SeqCst);
        RING_COUNT.fetch_add(1, Ordering::SeqCst);
    }
}

pub fn poll() -> Option<u8> {
    unsafe {
        if (inb(STATUS) & 1) == 0 {
            return None;
        }
        let sc = inb(DATA);
        if (sc & 0x80) != 0 {
            match sc & 0x7F {
                SC_LSHIFT | SC_RSHIFT => SHIFT = false,
                _ => {}
            }
            return None;
        }
        match sc {
            SC_LSHIFT | SC_RSHIFT => {
                SHIFT = true;
                return None;
            }
            SC_CAPSLOCK => {
                CAPS = !CAPS;
                return None;
            }
            _ => {}
        }
        let shifted = SHIFT ^ (CAPS && sc >= 0x10 && sc <= 0x2C);
        scancode_to_ascii(sc, shifted)
    }
}

pub fn read_byte() -> Option<u8> {
    if RING_COUNT.load(Ordering::SeqCst) == 0 {
        if let Some(c) = poll() {
            push_byte(c);
        }
    }
    if RING_COUNT.load(Ordering::SeqCst) == 0 {
        return None;
    }
    unsafe {
        let head = RING_HEAD.load(Ordering::SeqCst);
        let c = RING_BUF[head];
        RING_HEAD.store((head + 1) % BUF_SIZE, Ordering::SeqCst);
        RING_COUNT.fetch_sub(1, Ordering::SeqCst);
        Some(c)
    }
}

pub fn buffer_available() -> usize {
    BUF_SIZE - RING_COUNT.load(Ordering::SeqCst)
}
