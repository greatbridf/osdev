use eonix_mm::address::{PAddr, PRange};

pub const PLIC_BASE: PAddr = PAddr::from_val(0x0c00_0000);
pub const UART_BASE: PAddr = PAddr::from_val(0x1000_0000);
pub const RTC_BASE: PAddr = PAddr::from_val(0x0010_1000);
pub const UART_IRQ: usize = 10;

/// # Returns
/// (base, size)
pub fn virtio_devs() -> impl Iterator<Item = (PAddr, usize)> {
    let base = 0x1000_1000;
    (0..8).map(move |idx| (PAddr::from_val(base + idx * 0x1000), 0x1000))
}

pub fn present_ram() -> impl Iterator<Item = PRange> {
    core::iter::once(PRange::from(PAddr::from_val(0x80000000)).grow(0x40000000))
}

pub fn harts() -> impl Iterator<Item = usize> {
    0..1
}
