//! IDE PIO disk driver for QEMU. Primary master at 0x1F0.

use core::arch::asm;

const DATA: u16 = 0x1F0;
const SECTOR_COUNT: u16 = 0x1F2;
const LBA_LOW: u16 = 0x1F3;
const LBA_MID: u16 = 0x1F4;
const LBA_HIGH: u16 = 0x1F5;
const DRIVE: u16 = 0x1F6;
const STATUS: u16 = 0x1F7;
const CMD_READ: u8 = 0x20;
const CMD_WRITE: u8 = 0x30;
const STATUS_BSY: u8 = 0x80;
const STATUS_DRQ: u8 = 0x08;

#[inline(always)]
unsafe fn outb(port: u16, byte: u8) {
    asm!("out dx, al", in("dx") port, in("al") byte, options(nostack, preserves_flags));
}

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let ret: u8;
    asm!("in al, dx", in("dx") port, out("al") ret, options(nostack, preserves_flags));
    ret
}

fn wait_drq() -> bool {
    for _ in 0..1000000 {
        unsafe {
            let s = inb(STATUS);
            if (s & STATUS_BSY) == 0 && (s & STATUS_DRQ) != 0 {
                return true;
            }
        }
    }
    false
}

/// Read one 512-byte sector. LBA 0-based.
pub fn read_sector(lba: u32, buf: &mut [u8]) -> bool {
    if buf.len() < 512 {
        return false;
    }
    unsafe {
        outb(DRIVE, 0xE0 | ((lba >> 24) as u8 & 0x0F));
        outb(SECTOR_COUNT, 1);
        outb(LBA_LOW, (lba & 0xFF) as u8);
        outb(LBA_MID, ((lba >> 8) & 0xFF) as u8);
        outb(LBA_HIGH, ((lba >> 16) & 0xFF) as u8);
        outb(STATUS, CMD_READ);
        if !wait_drq() {
            return false;
        }
        for i in (0..512).step_by(2) {
            let word: u16;
            asm!("in ax, dx", in("dx") DATA, out("ax") word, options(nostack, preserves_flags));
            buf[i] = (word & 0xFF) as u8;
            buf[i + 1] = (word >> 8) as u8;
        }
    }
    true
}

/// Write one 512-byte sector.
pub fn write_sector(lba: u32, buf: &[u8]) -> bool {
    if buf.len() < 512 {
        return false;
    }
    unsafe {
        outb(DRIVE, 0xE0 | ((lba >> 24) as u8 & 0x0F));
        outb(SECTOR_COUNT, 1);
        outb(LBA_LOW, (lba & 0xFF) as u8);
        outb(LBA_MID, ((lba >> 8) & 0xFF) as u8);
        outb(LBA_HIGH, ((lba >> 16) & 0xFF) as u8);
        outb(STATUS, CMD_WRITE);
        if !wait_drq() {
            return false;
        }
        for i in (0..512).step_by(2) {
            let word = buf[i] as u16 | ((buf[i + 1] as u16) << 8);
            asm!("out dx, ax", in("dx") DATA, in("ax") word, options(nostack, preserves_flags));
        }
    }
    true
}
