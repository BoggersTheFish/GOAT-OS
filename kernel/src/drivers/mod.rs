//! Modular driver traits (Phase 3.3).

pub trait BlockDevice {
    fn read_sector(&self, lba: u32, buf: &mut [u8]) -> bool;
    fn write_sector(&self, lba: u32, buf: &[u8]) -> bool;
}

pub trait CharDevice {
    fn read_byte(&mut self) -> Option<u8>;
    fn write_byte(&mut self, b: u8);
    fn has_input(&self) -> bool;
}

pub mod net;


