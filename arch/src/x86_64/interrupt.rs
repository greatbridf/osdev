use core::{arch::asm, pin::Pin, ptr::NonNull};

use crate::rdmsr;

use super::pause;

/// Saved registers when a trap (interrupt or exception) occurs.
#[allow(missing_docs)]
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct InterruptContext {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rbp: u64,

    pub int_no: u64,
    pub error_code: u64,

    // Pushed by CPU
    pub rip: u64,
    pub cs: u64,
    pub eflags: u64,
    pub rsp: u64,
    pub ss: u64,
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

pub struct APICReg(*mut u32);
pub struct APICRegs {
    base: NonNull<u32>,
}

/// Architecture-specific interrupt control block.
pub struct InterruptControl {
    idt: [IDTEntry; 256],
    apic_base: APICRegs,
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
    pub fn local_apic_id(&self) -> APICReg {
        unsafe { APICReg::new(self.base.byte_offset(0x20).as_ptr()) }
    }

    pub fn task_priority(&self) -> APICReg {
        unsafe { APICReg::new(self.base.byte_offset(0x80).as_ptr()) }
    }

    pub fn end_of_interrupt(&self) {
        unsafe { APICReg::new(self.base.byte_offset(0xb0).as_ptr()).write(0) }
    }

    pub fn spurious(&self) -> APICReg {
        unsafe { APICReg::new(self.base.byte_offset(0xf0).as_ptr()) }
    }

    pub fn interrupt_command(&self) -> APICReg {
        unsafe { APICReg::new(self.base.byte_offset(0x300).as_ptr()) }
    }

    pub fn timer_register(&self) -> APICReg {
        unsafe { APICReg::new(self.base.byte_offset(0x320).as_ptr()) }
    }

    pub fn timer_initial_count(&self) -> APICReg {
        unsafe { APICReg::new(self.base.byte_offset(0x380).as_ptr()) }
    }

    pub fn timer_current_count(&self) -> APICReg {
        unsafe { APICReg::new(self.base.byte_offset(0x390).as_ptr()) }
    }

    pub fn timer_divide(&self) -> APICReg {
        unsafe { APICReg::new(self.base.byte_offset(0x3e0).as_ptr()) }
    }
}

impl InterruptControl {
    /// # Return
    /// Returns a tuple of InterruptControl and the cpu id of the current cpu.
    pub unsafe fn new() -> (Self, usize) {
        extern "C" {
            static ISR_START_ADDR: usize;
        }

        let idt = core::array::from_fn(|idx| match idx {
            0..0x80 => IDTEntry::new(unsafe { ISR_START_ADDR } + 8 * idx, 0x08, 0x8e),
            0x80 => IDTEntry::new(unsafe { ISR_START_ADDR } + 8 * idx, 0x08, 0xee),
            _ => IDTEntry::null(),
        });

        let apic_base = {
            let apic_base = rdmsr(0x1b);
            assert_eq!(apic_base & 0x800, 0x800, "LAPIC not enabled");

            let apic_base = ((apic_base & !0xfff) + 0xffffff00_00000000) as *mut u32;
            APICRegs {
                // TODO: A better way to convert to physical address
                base: NonNull::new(apic_base).expect("Invalid APIC base"),
            }
        };

        // Make sure APIC is enabled.
        apic_base.spurious().write(0x1ff);

        let cpuid = apic_base.local_apic_id().read() >> 24;

        (Self { idt, apic_base }, cpuid as usize)
    }

    pub fn setup_timer(&self) {
        self.apic_base.task_priority().write(0);
        self.apic_base.timer_divide().write(0x3); // Divide by 16
        self.apic_base.timer_register().write(0x20040);

        // TODO: Get the bus frequency from...?
        let freq = 200;
        let count = freq * 1_000_000 / 16 / 100;
        self.apic_base.timer_initial_count().write(count as u32);
    }

    pub fn setup_idt(self: Pin<&mut Self>) {
        lidt(
            self.idt.as_ptr() as usize,
            (size_of::<IDTEntry>() * 256 - 1) as u16,
        );
    }

    pub fn send_sipi(&self) {
        let icr = self.apic_base.interrupt_command();

        icr.write(0xc4500);
        while icr.read() & 0x1000 != 0 {
            pause();
        }

        icr.write(0xc4601);
        while icr.read() & 0x1000 != 0 {
            pause();
        }
    }

    /// Send EOI to APIC so that it can send more interrupts.
    pub fn end_of_interrupt(&self) {
        self.apic_base.end_of_interrupt()
    }
}

pub fn enable_irqs() {
    unsafe {
        asm!("sti");
    }
}

pub fn disable_irqs() {
    unsafe {
        asm!("cli");
    }
}

fn lidt(base: usize, limit: u16) {
    let mut idt_descriptor = [0u16; 5];

    idt_descriptor[0] = limit;
    idt_descriptor[1] = base as u16;
    idt_descriptor[2] = (base >> 16) as u16;
    idt_descriptor[3] = (base >> 32) as u16;
    idt_descriptor[4] = (base >> 48) as u16;

    unsafe {
        asm!("lidt ({})", in(reg) &idt_descriptor, options(att_syntax));
    }
}
