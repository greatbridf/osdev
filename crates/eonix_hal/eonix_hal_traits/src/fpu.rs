#[doc(notable_trait)]
pub trait RawFpuState {
    fn save(&mut self);
    fn restore(&mut self);
}
