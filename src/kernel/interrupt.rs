use super::cpu::local_cpu;
use super::mem::handle_page_fault;
use super::syscall::handle_syscall32;
use super::task::{ProcessList, Signal, Thread};
use super::timer::timer_interrupt;
use crate::bindings::root::EINVAL;
use crate::{driver::Port8, prelude::*};
use alloc::sync::Arc;
use arch::{ExtendedContext, InterruptContext};
use eonix_runtime::task::Task;
use eonix_sync::SpinIrq as _;

const PIC1_COMMAND: Port8 = Port8::new(0x20);
const PIC1_DATA: Port8 = Port8::new(0x21);
const PIC2_COMMAND: Port8 = Port8::new(0xA0);
const PIC2_DATA: Port8 = Port8::new(0xA1);

static IRQ_HANDLERS: Spin<[Option<Arc<dyn Fn() + Send + Sync>>; 16]> =
    Spin::new([const { None }; 16]);

fn irq_handler(irqno: usize) {
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

fn fault_handler(int_stack: &mut InterruptContext) {
    match int_stack.int_no {
        // Invalid Op or Double Fault
        14 => handle_page_fault(int_stack),
        13 if int_stack.cs & 0x3 != 0 => ProcessList::kill_current(Signal::SIGILL),
        6 | 8 if int_stack.cs & 0x3 != 0 => ProcessList::kill_current(Signal::SIGSEGV),
        _ => panic!("Unhandled fault: {}", int_stack.int_no),
    }
}

#[eonix_hal::default_trap_handler]
pub extern "C" fn interrupt_handler(
    int_stack: *mut InterruptContext,
    ext_ctx: *mut ExtendedContext,
) {
    let int_stack = unsafe { &mut *int_stack };
    let ext_ctx = unsafe { &mut *ext_ctx };

    match int_stack.int_no {
        // Fault
        0..0x20 => fault_handler(int_stack),
        // Syscall
        0x80 => handle_syscall32(int_stack.rax as usize, int_stack, ext_ctx),
        // Timer
        0x40 => timer_interrupt(),
        // IRQ
        no => irq_handler(no as usize - 0x20),
    }

    if int_stack.cs & 0x3 != 0 {
        if Thread::current().signal_list.has_pending_signal() {
            Task::block_on(Thread::current().signal_list.handle(int_stack, ext_ctx));
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
