pub trait Relax {
    fn relax();
}

#[derive(Default, Debug, Clone, Copy)]
pub struct LoopRelax;
impl Relax for LoopRelax {
    fn relax() {}
}

#[derive(Default, Debug, Clone, Copy)]
pub struct SpinRelax;
impl Relax for SpinRelax {
    fn relax() {
        core::hint::spin_loop();
    }
}
