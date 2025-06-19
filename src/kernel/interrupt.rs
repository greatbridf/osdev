use super::mem::handle_kernel_page_fault;
use super::timer::timer_interrupt;
use crate::kernel::constants::EINVAL;
use crate::prelude::*;
use alloc::sync::Arc;
use eonix_hal::processor::CPU;
use eonix_hal::traits::fault::Fault;
use eonix_hal::traits::trap::{RawTrapContext, TrapType};
use eonix_hal::trap::TrapContext;
use eonix_mm::address::{Addr as _, VAddr};
use eonix_runtime::scheduler::Scheduler;
use eonix_sync::SpinIrq as _;

static IRQ_HANDLERS: Spin<[Option<Arc<dyn Fn() + Send + Sync>>; 16]> =
    Spin::new([const { None }; 16]);

pub fn default_irq_handler(irqno: usize) {
    assert!(irqno < 16);

    let handler = IRQ_HANDLERS.lock()[irqno as usize].as_ref().cloned();
    if let Some(handler) = handler {
        handler();
    }

    #[cfg(target_arch = "x86_64")]
    {
        use eonix_hal::arch_exported::io::Port8;

        const PIC1_COMMAND: Port8 = Port8::new(0x20);
        const PIC2_COMMAND: Port8 = Port8::new(0xA0);

        PIC1_COMMAND.write(0x20); // EOI
        if irqno >= 8 {
            PIC2_COMMAND.write(0x20); // EOI
        }
    }
}

pub fn default_fault_handler(fault_type: Fault, trap_ctx: &mut TrapContext) {
    if trap_ctx.is_user_mode() {
        unimplemented!("Unhandled user space fault");
    }

    match fault_type {
        Fault::PageFault {
            error_code,
            address: vaddr,
        } => {
            let fault_pc = VAddr::from(trap_ctx.get_program_counter());

            if let Some(new_pc) = handle_kernel_page_fault(fault_pc, vaddr, error_code) {
                trap_ctx.set_program_counter(new_pc.addr());
            }
        }
        fault => panic!("Unhandled kernel space fault: {fault:?}"),
    }
}

#[eonix_hal::default_trap_handler]
pub fn interrupt_handler(trap_ctx: &mut TrapContext) {
    match trap_ctx.trap_type() {
        TrapType::Syscall { no, .. } => unreachable!("Syscall {} in kernel space.", no),
        TrapType::Fault(fault) => default_fault_handler(fault, trap_ctx),
        TrapType::Irq(no) => default_irq_handler(no),
        TrapType::Timer => {
            timer_interrupt();

            if eonix_preempt::count() == 0 {
                // To make scheduler satisfied.
                eonix_preempt::disable();
                Scheduler::schedule();
            }
        }
    }
}

pub fn register_irq_handler<F>(irqno: i32, handler: F) -> Result<(), u32>
where
    F: Fn() + Send + Sync + 'static,
{
    if irqno < 0 || irqno >= 16 {
        return Err(EINVAL);
    }

    let old = IRQ_HANDLERS.lock_irq()[irqno as usize].replace(Arc::new(handler));
    assert!(old.is_none(), "IRQ handler already registered");
    Ok(())
}

pub fn end_of_interrupt() {
    CPU::local().as_mut().end_of_interrupt();
}
