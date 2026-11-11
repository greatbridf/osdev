pub trait RawFpuState: Copy {
    fn new() -> Self;
    fn save(&mut self);
    fn restore(&mut self);
}
