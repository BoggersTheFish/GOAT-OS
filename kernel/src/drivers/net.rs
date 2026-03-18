//! Networking stub (Phase 3.4).
//! Placeholder for virtio-net/e1000 driver and basic UDP.

pub fn init() {
    // TODO: PCI scan + virtio-net/e1000 init
}

pub fn send_udp(_dst_ip: [u8; 4], _dst_port: u16, _payload: &[u8]) -> bool {
    // TODO: minimal UDP send once driver exists
    false
}

