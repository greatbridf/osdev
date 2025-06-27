use eonix_mm::address::PRange;

pub trait NetBuffer {
    fn as_phys(&self) -> PRange;
}
