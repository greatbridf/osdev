mod trap_context;

use eonix_hal_traits::{context::RawTaskContext, trap::TrapReturn};
pub use trap_context::*;

use riscv::{
    asm::sfence_vma_all,
    register::{
        sstatus::{self, Sstatus},
        stvec::{self, Stvec}
    }
};
use sbi::SbiError;
use core::arch::{global_asm, naked_asm};

use super::context::TaskContext;

use super::config::platform::virt::*;

//global_asm!(include_str!("trap.S"));

use riscv::register::{scause, sepc, stval};

//#[eonix_percpu::define_percpu]
//static TRAP_HANDLER: unsafe extern "C" fn() = default_trap_handler;

#[eonix_percpu::define_percpu]
static TRAP_HANDLER: unsafe extern "C" fn() = default_trap_handler;

#[eonix_percpu::define_percpu]
static CAPTURER_CONTEXT: TaskContext = TaskContext::new();

/// This value will never be used.
static mut DIRTY_TRAP_CONTEXT: TaskContext = TaskContext::new();

// TODO: is need to save kernel's callee saved registers?
global_asm!(
    r"
    .altmacro
    .macro SAVE_GP n
        sd x\n, \n*8(sp)
    .endm
    .macro LOAD_GP n
        ld x\n, \n*8(sp)
    .endm

    .section .text
        .globl _raw_trap_entry
        .globl return_to_user
        .align 2

    _raw_trap_entry:
        # swap sp and sscratch(previously stored user TrapContext's address in return_to_user)
        csrrw sp, sscratch, sp

        sd x1, 1*8(sp)
        .set n, 3
        .rept 29
            SAVE_GP %n
            .set n, n+1
        .endr

        csrr t0, sstatus
        csrr t1, sepc
        csrr t2, scause
        csrr t3, stval
        sd t0, 32*8(sp)     # save sstatus into the TrapContext
        sd t1, 33*8(sp)     # save sepc into the TrapContext
        sd t2, 34*8(sp)     # save scause into the TrapContext
        sd t3, 35*8(sp)     # save stval into the TrapContext

        csrr t0, sscratch
        sd t0, 2*8(sp)      # save user stack pointer into the TrapContext

        addi t0, tp, {handler}
        ld t1, 0(t0)
        jr t1

    _raw_trap_return:
        # sscratch store the TrapContext's address
        csrw sscratch, a0

        mv sp, a0
        # now sp points to TrapContext in kernel space

        # restore sstatus and sepc
        ld t0, 32*8(sp)
        ld t1, 33*8(sp)
        ld t2, 34*8(sp)
        ld t3, 35*8(sp)
        csrw sstatus, t0
        csrw sepc, t1
        csrw scause, t2
        csrw stval, t3

        # save x* expect x0 and sp
        ld x1, 1*8(sp)
        .set n, 3
        .rept 29
            LOAD_GP %n
            .set n, n+1
        .endr
        ld sp, 2*8(sp)

        sret
    ",
    handler = sym _percpu_inner_TRAP_HANDLER,

);

unsafe extern "C" {
    fn _default_trap_handler(trap_context: &mut TrapContext);
    fn _raw_trap_entry();
    fn _raw_trap_return();
}

/// TODO:
/// default_trap_handler
/// captured_trap_handler
/// _raw_trap_entry应该是做好了
/// _raw_trap_return应该是做好了
#[unsafe(naked)]
unsafe extern "C" fn default_trap_handler() {
    naked_asm!(
        "mv t0, sp",
        "andi sp, sp, -16",
        "mv a0, t0",
        "call {handle_trap}",

        "mv sp, t0",

        "j {trap_return}",

        handle_trap = sym _default_trap_handler,
        trap_return = sym _raw_trap_return,
    );
}

#[unsafe(naked)]
unsafe extern "C" fn captured_trap_handler() {
    naked_asm!(
        "addi sp, sp, -16",
        "sd ra, 8(sp)",

        "la a0, {from_context}",

        "mv t0, tp",
        "la t1, {to_context}",
        "add a1, t0, t1",

        "call {switch}",

        "ld ra, 8(sp)",
        "addi sp, sp, 16",
        "ret",

        from_context = sym DIRTY_TRAP_CONTEXT,
        to_context = sym _percpu_inner_CAPTURER_CONTEXT,
        switch = sym TaskContext::switch,
    );
}

#[unsafe(naked)]
unsafe extern "C" fn captured_trap_return(trap_context: usize) -> ! {
    naked_asm!(
        "la t0, {trap_return}",
        "jalr zero, t0, 0",
        trap_return = sym _raw_trap_return,
    );
}

impl TrapReturn for TrapContext {
    unsafe fn trap_return(&mut self) {
        let irq_states = disable_irqs_save();
        let old_handler = TRAP_HANDLER.swap(captured_trap_handler);

        let mut to_ctx = TaskContext::new();
        to_ctx.set_program_counter(captured_trap_return as _);
        to_ctx.set_stack_pointer(&raw mut *self as usize);
        to_ctx.set_interrupt_enabled(false);

        unsafe {
            TaskContext::switch(CAPTURER_CONTEXT.as_mut(), &mut to_ctx);
        }

        TRAP_HANDLER.set(old_handler);
        irq_states.restore();
    }
}

fn setup_trap_handler(trap_entry_addr: usize) {
    unsafe {
        stvec::write(Stvec::from_bits(trap_entry_addr));
    }
    sfence_vma_all();
}

pub fn setup_trap() {
    setup_trap_handler(_raw_trap_entry as usize);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqState(usize);

impl IrqState {
    #[inline]
    pub fn save() -> Self {
        let sstatus_val = sstatus::read().bits();

        unsafe {
            sstatus::clear_sie();
        }

        IrqState(sstatus_val)
    }

    #[inline]
    pub fn restore(self) {
        let Self(state) = self;
        unsafe {
            sstatus::write(Sstatus::from_bits(state));
        }
    }

    #[inline]
    pub fn was_enabled(&self) -> bool {
        (self.0 & (1 << 1)) != 0
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
    unsafe {
        let original_sstatus_bits = sstatus::read().bits();
        sstatus::clear_sie();

        IrqState(original_sstatus_bits)
    }
}
