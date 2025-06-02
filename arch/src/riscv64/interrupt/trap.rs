use super::setup_kernel_trap;

use riscv::register::{scause, sepc, stval};

/// TODO:
/// kernel_trap_entry, 暂时不知道要干什么，大概是处理一下原因，然后跳到default handler
/// user_trap_entry, 大概是一样的
/// trap_return, 估计不是在这里写，还不太清楚

#[no_mangle]
pub fn kernel_trap_entry() {
    let stval = stval::read();
    let scause = scause::read();
    let sepc = sepc::read();
    let cause = scause.cause();

    match scause.cause() {
        scause::Trap::Interrupt(_) => todo!(),
        scause::Trap::Exception(_) => todo!(),
    }
}

#[no_mangle]
pub fn user_trap_entry() {
    setup_kernel_trap();

}

#[no_mangle]
pub fn trap_return() {

}
