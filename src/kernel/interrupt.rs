use alloc::sync::Arc;

use lazy_static::lazy_static;

use crate::bindings::root::{interrupt_stack, mmx_registers, EINVAL};
use crate::{driver::Port8, prelude::*};

use super::mem::handle_page_fault;
use super::syscall::handle_syscall32;
use super::task::{ProcessList, Signal};

const PIC1_COMMAND: Port8 = Port8::new(0x20);
const PIC1_DATA: Port8 = Port8::new(0x21);
const PIC2_COMMAND: Port8 = Port8::new(0xA0);
const PIC2_DATA: Port8 = Port8::new(0xA1);

#[repr(C)]
#[derive(Clone, Copy)]
struct IDTEntry {
    offset_low: u16,
    selector: u16,

    interrupt_stack: u8,
    attributes: u8,

    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

extern "C" {
    static ISR_START_ADDR: usize;
}

lazy_static! {
    static ref IRQ_HANDLERS: Spin<[Option<Arc<dyn Fn() + Send + Sync>>; 16]> =
        Spin::new([const { None }; 16]);
    static ref IDT: [IDTEntry; 256] = core::array::from_fn(|idx| {
        match idx {
            0..0x80 => IDTEntry::new(unsafe { ISR_START_ADDR } + 8 * idx, 0x08, 0x8e),
            0x80 => IDTEntry::new(unsafe { ISR_START_ADDR } + 8 * idx, 0x08, 0xee),
            _ => IDTEntry::null(),
        }
    });
}

impl IDTEntry {
    const fn new(offset: usize, selector: u16, attributes: u8) -> Self {
        Self {
            offset_low: offset as u16,
            selector,
            interrupt_stack: 0,
            attributes,
            offset_mid: (offset >> 16) as u16,
            offset_high: (offset >> 32) as u32,
            reserved: 0,
        }
    }

    const fn null() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            interrupt_stack: 0,
            attributes: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }
}

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

fn fault_handler(int_stack: &mut interrupt_stack) {
    match int_stack.int_no {
        // Invalid Op or Double Fault
        14 => handle_page_fault(int_stack),
        13 if int_stack.ss == 0 => ProcessList::kill_current(Signal::SIGILL),
        6 | 8 if int_stack.ss == 0 => ProcessList::kill_current(Signal::SIGSEGV),
        _ => panic!("Unhandled fault: {}", int_stack.int_no),
    }
}

#[no_mangle]
pub extern "C" fn interrupt_handler(int_stack: *mut interrupt_stack, mmxregs: *mut mmx_registers) {
    let int_stack = unsafe { &mut *int_stack };
    let mmxregs = unsafe { &mut *mmxregs };

    match int_stack.int_no {
        // Fault
        0..0x20 => fault_handler(int_stack),
        // Syscall
        0x80 => handle_syscall32(int_stack.regs.rax as usize, int_stack, mmxregs),
        // IRQ
        no => irq_handler(no as usize - 0x20),
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
    arch::x86_64::interrupt::lidt(
        IDT.as_ptr() as usize,
        (size_of::<IDTEntry>() * 256 - 1) as u16,
    );

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
