//! VGA text mode driver (0xB8000, 80x25)

use core::sync::atomic::{AtomicUsize, Ordering};

const WIDTH: usize = 80;
const HEIGHT: usize = 25;
const VGA_BASE: *mut u16 = 0xB8000 as *mut u16;

const COLOR_FG: u8 = 0x07;
const COLOR_BG: u8 = 0x00;
const ATTRIBUTE: u8 = (COLOR_BG << 4) | COLOR_FG;

static ROW: AtomicUsize = AtomicUsize::new(0);
static COL: AtomicUsize = AtomicUsize::new(0);

#[inline(always)]
unsafe fn outb(port: u16, byte: u8) {
    core::arch::asm!("out dx, al", in("dx") port, in("al") byte, options(nostack, preserves_flags));
}

pub fn init() {
    unsafe {
        outb(0x3D4, 0x0A);
        outb(0x3D5, 0x20);
        outb(0x3D4, 0x0B);
        outb(0x3D5, 0x00);
    }
    clear();
}

pub fn clear() {
    let attr = (ATTRIBUTE as u16) << 8;
    unsafe {
        for i in 0..(WIDTH * HEIGHT) {
            *VGA_BASE.add(i) = b' ' as u16 | attr;
        }
    }
    ROW.store(0, Ordering::SeqCst);
    COL.store(0, Ordering::SeqCst);
}

fn newline() {
    let r = ROW.load(Ordering::SeqCst);
    if r + 1 >= HEIGHT {
        scroll();
        ROW.store(HEIGHT - 1, Ordering::SeqCst);
    } else {
        ROW.store(r + 1, Ordering::SeqCst);
    }
    COL.store(0, Ordering::SeqCst);
}

fn scroll() {
    let attr = (ATTRIBUTE as u16) << 8;
    unsafe {
        for row in 0..(HEIGHT - 1) {
            for col in 0..WIDTH {
                let src = (row + 1) * WIDTH + col;
                let dst = row * WIDTH + col;
                *VGA_BASE.add(dst) = *VGA_BASE.add(src);
            }
        }
        for col in 0..WIDTH {
            *VGA_BASE.add((HEIGHT - 1) * WIDTH + col) = b' ' as u16 | attr;
        }
    }
}

pub fn write_byte(b: u8) {
    match b {
        b'\n' => newline(),
        b'\r' => COL.store(0, Ordering::SeqCst),
        _ => {
            let r = ROW.load(Ordering::SeqCst);
            let c = COL.load(Ordering::SeqCst);
            if c >= WIDTH {
                newline();
                write_byte(b);
                return;
            }
            let attr = (ATTRIBUTE as u16) << 8;
            unsafe {
                *VGA_BASE.add(r * WIDTH + c) = (b as u16) | attr;
            }
            COL.store(c + 1, Ordering::SeqCst);
        }
    }
}

pub fn write_str(s: &str) {
    for b in s.bytes() {
        write_byte(b);
    }
}
