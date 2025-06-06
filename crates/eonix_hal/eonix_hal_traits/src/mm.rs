use eonix_mm::address::PRange;

pub trait Memory {
    fn present_ram() -> impl Iterator<Item = PRange>;
    fn free_ram() -> impl Iterator<Item = PRange>;
}
