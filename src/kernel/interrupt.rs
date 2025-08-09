use super::mem::handle_kernel_page_fault;
use super::timer::timer_interrupt;
use crate::kernel::constants::EINVAL;
use crate::prelude::*;
use alloc::sync::Arc;
use eonix_hal::traits::fault::Fault;
use eonix_hal::traits::trap::{RawTrapContext, TrapType};
use eonix_hal::trap::TrapContext;
use eonix_mm::address::{Addr as _, VAddr};
use eonix_sync::SpinIrq as _;

static IRQ_HANDLERS: Spin<[Vec<Arc<dyn Fn() + Send + Sync>>; 16]> =
    Spin::new([const { Vec::new() }; 16]);

pub fn default_irq_handler(irqno: usize) {
    assert!(irqno < 16);

    {
        let handlers = IRQ_HANDLERS.lock();

        for handler in handlers[irqno].iter() {
            handler();
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
        TrapType::Breakpoint => unreachable!("Breakpoint in kernel space."),
        TrapType::Fault(fault) => default_fault_handler(fault, trap_ctx),
        TrapType::Irq { callback } => callback(default_irq_handler),
        TrapType::Timer { callback } => callback(timer_interrupt),
    }
}

pub fn register_irq_handler<F>(irqno: i32, handler: F) -> Result<(), u32>
where
    F: Fn() + Send + Sync + 'static,
{
    if irqno < 0 || irqno >= 16 {
        return Err(EINVAL);
    }

    IRQ_HANDLERS.lock_irq()[irqno as usize].push(Arc::new(handler));
    Ok(())
}
