use crate::kernel::mem::phys::{CachedPP, PhysPtr as _};
use arch::task::rdmsr;
use lazy_static::lazy_static;

extern "C" {
    static ISR_START_ADDR: usize;
}

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

pub struct APICReg(*mut u32);
pub struct APICRegs {
    base: CachedPP,
}

impl APICReg {
    fn new(pointer: *mut u32) -> Self {
        Self(pointer)
    }

    pub fn read(&self) -> u32 {
        unsafe { self.0.read_volatile() }
    }

    pub fn write(&self, value: u32) {
        unsafe { self.0.write_volatile(value) }
    }
}

impl APICRegs {
    pub fn spurious(&self) -> APICReg {
        APICReg::new(self.base.offset(0xf0).as_ptr())
    }

    pub fn task_priority(&self) -> APICReg {
        APICReg::new(self.base.offset(0x80).as_ptr())
    }

    pub fn end_of_interrupt(&self) {
        APICReg::new(self.base.offset(0xb0).as_ptr()).write(0)
    }

    pub fn interrupt_command(&self) -> APICReg {
        APICReg::new(self.base.offset(0x300).as_ptr())
    }

    pub fn timer_register(&self) -> APICReg {
        APICReg::new(self.base.offset(0x320).as_ptr())
    }

    pub fn timer_initial_count(&self) -> APICReg {
        APICReg::new(self.base.offset(0x380).as_ptr())
    }

    pub fn timer_current_count(&self) -> APICReg {
        APICReg::new(self.base.offset(0x390).as_ptr())
    }

    pub fn timer_divide(&self) -> APICReg {
        APICReg::new(self.base.offset(0x3e0).as_ptr())
    }
}

lazy_static! {
    static ref IDT: [IDTEntry; 256] = core::array::from_fn(|idx| match idx {
        0..0x80 => IDTEntry::new(unsafe { ISR_START_ADDR } + 8 * idx, 0x08, 0x8e),
        0x80 => IDTEntry::new(unsafe { ISR_START_ADDR } + 8 * idx, 0x08, 0xee),
        _ => IDTEntry::null(),
    });
    pub static ref APIC_BASE: APICRegs = {
        let apic_base = rdmsr(0x1b);
        assert_eq!(apic_base & 0x800, 0x800, "LAPIC not enabled");
        assert_eq!(apic_base & 0x100, 0x100, "Is not bootstrap processor");

        let apic_base = apic_base & !0xfff;
        APICRegs {
            base: CachedPP::new(apic_base as usize),
        }
    };
}

pub fn setup_idt() {
    arch::x86_64::interrupt::lidt(
        IDT.as_ptr() as usize,
        (size_of::<IDTEntry>() * 256 - 1) as u16,
    );
}

pub fn end_of_interrupt() {
    APIC_BASE.end_of_interrupt()
}
