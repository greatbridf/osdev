use riscv::register::time;
use super::config::{
    platform::virt::CPU_FREQ_HZ,
    time::INTERRUPTS_PER_SECOND
};

pub fn get_time() -> usize {
    time::read()
}

pub fn set_timer(times: usize) {
    let time = (get_time() + times * CPU_FREQ_HZ as usize / INTERRUPTS_PER_SECOND) as u64;
    sbi::legacy::set_timer(time);
}

pub fn set_next_timer() {
    set_timer(1);
}
