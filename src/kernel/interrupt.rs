use super::cpu::local_cpu;
use super::mem::handle_kernel_page_fault;
use super::timer::timer_interrupt;
use crate::bindings::root::EINVAL;
use crate::{driver::Port8, prelude::*};
use alloc::sync::Arc;
use arch::TrapContext;
use eonix_hal::traits::fault::Fault;
use eonix_hal::traits::trap::{RawTrapContext, TrapType};
use eonix_mm::address::{Addr as _, VAddr};
use eonix_runtime::scheduler::Scheduler;
use eonix_sync::SpinIrq as _;

const PIC1_COMMAND: Port8 = Port8::new(0x20);
const PIC1_DATA: Port8 = Port8::new(0x21);
const PIC2_COMMAND: Port8 = Port8::new(0xA0);
const PIC2_DATA: Port8 = Port8::new(0xA1);

static IRQ_HANDLERS: Spin<[Option<Arc<dyn Fn() + Send + Sync>>; 16]> =
    Spin::new([const { None }; 16]);

pub fn default_irq_handler(irqno: usize) {
    assert!(irqno < 16);

    let handler = IRQ_HANDLERS.lock()[irqno as usize].as_ref().cloned();
    if let Some(handler) = handler {
        handler();
    }

    PIC1_COMMAND.write(0x20); // EOI
    if irqno >= 8 {
        PIC2_COMMAND.write(0x20); // EOI
    }
}

pub fn default_fault_handler(fault_type: Fault, trap_ctx: &mut TrapContext) {
    if trap_ctx.is_user_mode() {
        unimplemented!("Unhandled user space fault");
    }

    match fault_type {
        Fault::PageFault(error_code) => {
            let fault_pc = VAddr::from(trap_ctx.get_program_counter());
            let vaddr = arch::get_page_fault_address();

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

pub fn init() -> KResult<()> {
    // Initialize PIC
    PIC1_COMMAND.write(0x11); // edge trigger mode
    PIC1_DATA.write(0x20); // IRQ 0-7 offset
    PIC1_DATA.write(0x04); // cascade with slave PIC
    PIC1_DATA.write(0x01); // no buffer mode

    PIC2_COMMAND.write(0x11); // edge trigger mode
    PIC2_DATA.write(0x28); // IRQ 8-15 offset
    PIC2_DATA.write(0x02); // cascade with master PIC
    PIC2_DATA.write(0x01); // no buffer mode

    // Allow all IRQs
    PIC1_DATA.write(0x0);
    PIC2_DATA.write(0x0);

    Ok(())
}

pub fn end_of_interrupt() {
    // SAFETY: We only use this function in irq context, where preemption is disabled.
    unsafe { local_cpu() }.interrupt.end_of_interrupt();
}
