mod captured;
mod default;
mod trap_context;

use super::config::platform::virt::*;
use super::context::TaskContext;
use captured::{_captured_trap_entry, _captured_trap_return};
use core::arch::{global_asm, naked_asm};
use core::mem::{offset_of, size_of};
use core::num::NonZero;
use core::ptr::NonNull;
use default::_default_trap_entry;
use eonix_hal_traits::{
    context::RawTaskContext,
    trap::{IrqState as IrqStateTrait, TrapReturn},
};
use riscv::register::sstatus::{self, Sstatus};
use riscv::register::stvec::TrapMode;
use riscv::register::{scause, sepc, sscratch, stval};
use riscv::{
    asm::sfence_vma_all,
    register::stvec::{self, Stvec},
};
use sbi::SbiError;

pub use trap_context::*;

impl TrapReturn for TrapContext {
    type TaskContext = TaskContext;

    unsafe fn trap_return(&mut self) {
        let irq_states = disable_irqs_save();

        let old_stvec = stvec::read();
        stvec::write({
            let mut stvec_val = Stvec::from_bits(0);
            stvec_val.set_address(_captured_trap_entry as usize);
            stvec_val.set_trap_mode(TrapMode::Direct);
            stvec_val
        });

        let old_trap_ctx = sscratch::read();
        sscratch::write(&raw mut *self as usize);

        let mut from_ctx = TaskContext::new();
        let mut to_ctx = TaskContext::new();
        to_ctx.set_program_counter(_captured_trap_return as usize);
        to_ctx.set_stack_pointer(&raw mut from_ctx as usize);
        to_ctx.set_interrupt_enabled(false);

        unsafe {
            TaskContext::switch(&mut from_ctx, &mut to_ctx);
        }

        sscratch::write(old_trap_ctx);
        stvec::write(old_stvec);

        irq_states.restore();
    }
}

fn setup_trap_handler(trap_entry_addr: usize) {
    let mut stvec_val = Stvec::from_bits(0);
    stvec_val.set_address(trap_entry_addr);
    stvec_val.set_trap_mode(TrapMode::Direct);

    unsafe {
        stvec::write(stvec_val);
    }
}

pub fn setup_trap() {
    setup_trap_handler(_default_trap_entry as usize);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqState(Sstatus);

impl IrqState {
    #[inline]
    pub fn save() -> Self {
        IrqState(sstatus::read())
    }
}

impl IrqStateTrait for IrqState {
    fn restore(self) {
        let Self(state) = self;
        unsafe {
            sstatus::write(state);
        }
    }
}

#[inline]
pub fn disable_irqs() {
    unsafe {
        sstatus::clear_sie();
    }
}

#[inline]
pub fn enable_irqs() {
    unsafe {
        sstatus::set_sie();
    }
}

#[inline]
pub fn disable_irqs_save() -> IrqState {
    let state = IrqState::save();
    disable_irqs();

    state
}
