use core::{
    arch::{asm, global_asm},
    pin::Pin,
    ptr::NonNull,
};

use crate::rdmsr;

use super::pause;

global_asm!(
    r"
    .set RAX, 0x00
    .set RBX, 0x08
    .set RCX, 0x10
    .set RDX, 0x18
    .set RDI, 0x20
    .set RSI, 0x28
    .set R8, 0x30
    .set R9, 0x38
    .set R10, 0x40
    .set R11, 0x48
    .set R12, 0x50
    .set R13, 0x58
    .set R14, 0x60
    .set R15, 0x68
    .set RBP, 0x70
    .set INT_NO, 0x78
    .set ERRCODE, 0x80
    .set RIP, 0x88
    .set CS, 0x90
    .set FLAGS, 0x98
    .set RSP, 0xa0
    .set SS, 0xa8

    .macro movcfi reg, offset
        mov \reg, \offset(%rsp)
        .cfi_rel_offset \reg, \offset
    .endm

    .macro movrst reg, offset
        mov \offset(%rsp), \reg
        .cfi_restore \reg
    .endm

    .globl ISR_stub_restore
    .type ISR_stub_restore @function

    ISR_stub:
        .cfi_startproc
        .cfi_signal_frame
        .cfi_def_cfa_offset 0x18
        .cfi_offset %rsp, 0x10

        cmpq $0x08, 24(%rsp)
        je 1f
        swapgs

    1:
        sub $0x78, %rsp
        .cfi_def_cfa_offset 0x90

        movcfi %rax, RAX
        movcfi %rbx, RBX
        movcfi %rcx, RCX
        movcfi %rdx, RDX
        movcfi %rdi, RDI
        movcfi %rsi, RSI
        movcfi %r8,  R8
        movcfi %r9,  R9
        movcfi %r10, R10
        movcfi %r11, R11
        movcfi %r12, R12
        movcfi %r13, R13
        movcfi %r14, R14
        movcfi %r15, R15
        movcfi %rbp, RBP

        mov INT_NO(%rsp), %rax
        sub $ISR0, %rax
        shr $3, %rax
        mov %rax, INT_NO(%rsp)

        mov %rsp, %rbx
        .cfi_def_cfa_register %rbx

        and $~0xf, %rsp
        sub $512, %rsp
        fxsave (%rsp)

        mov %rbx, %rdi
        mov %rsp, %rsi
        call interrupt_handler

    ISR_stub_restore:
        fxrstor (%rsp)
        mov %rbx, %rsp
        .cfi_def_cfa_register %rsp

    .globl _arch_fork_return
    _arch_fork_return:
        movrst %rax, RAX
        movrst %rbx, RBX
        movrst %rcx, RCX
        movrst %rdx, RDX
        movrst %rdi, RDI
        movrst %rsi, RSI
        movrst %r8,  R8
        movrst %r9,  R9
        movrst %r10, R10
        movrst %r11, R11
        movrst %r12, R12
        movrst %r13, R13
        movrst %r14, R14
        movrst %r15, R15
        movrst %rbp, RBP

        add $0x88, %rsp
        .cfi_def_cfa_offset 0x08

        cmpq $0x08, 8(%rsp)
        je 1f
        swapgs

    1:
        iretq
        .cfi_endproc

    .altmacro
    .macro build_isr_no_err name
        .align 8
        .globl ISR\name
        .type  ISR\name @function
        ISR\name:
            .cfi_startproc
            .cfi_signal_frame
            .cfi_def_cfa_offset 0x08
            .cfi_offset %rsp, 0x10

            .cfi_same_value %rax
            .cfi_same_value %rbx
            .cfi_same_value %rcx
            .cfi_same_value %rdx
            .cfi_same_value %rdi
            .cfi_same_value %rsi
            .cfi_same_value %r8
            .cfi_same_value %r9
            .cfi_same_value %r10
            .cfi_same_value %r11
            .cfi_same_value %r12
            .cfi_same_value %r13
            .cfi_same_value %r14
            .cfi_same_value %r15
            .cfi_same_value %rbp

            push %rbp # push placeholder for error code
            .cfi_def_cfa_offset 0x10

            call ISR_stub
            .cfi_endproc
    .endm

    .altmacro
    .macro build_isr_err name
        .align 8
        .globl ISR\name
        .type  ISR\name @function
        ISR\name:
            .cfi_startproc
            .cfi_signal_frame
            .cfi_def_cfa_offset 0x10
            .cfi_offset %rsp, 0x10

            .cfi_same_value %rax
            .cfi_same_value %rbx
            .cfi_same_value %rcx
            .cfi_same_value %rdx
            .cfi_same_value %rdi
            .cfi_same_value %rsi
            .cfi_same_value %r8
            .cfi_same_value %r9
            .cfi_same_value %r10
            .cfi_same_value %r11
            .cfi_same_value %r12
            .cfi_same_value %r13
            .cfi_same_value %r14
            .cfi_same_value %r15
            .cfi_same_value %rbp

            call ISR_stub
            .cfi_endproc
    .endm

    build_isr_no_err 0
    build_isr_no_err 1
    build_isr_no_err 2
    build_isr_no_err 3
    build_isr_no_err 4
    build_isr_no_err 5
    build_isr_no_err 6
    build_isr_no_err 7
    build_isr_err    8
    build_isr_no_err 9
    build_isr_err    10
    build_isr_err    11
    build_isr_err    12
    build_isr_err    13
    build_isr_err    14
    build_isr_no_err 15
    build_isr_no_err 16
    build_isr_err    17
    build_isr_no_err 18
    build_isr_no_err 19
    build_isr_no_err 20
    build_isr_err    21
    build_isr_no_err 22
    build_isr_no_err 23
    build_isr_no_err 24
    build_isr_no_err 25
    build_isr_no_err 26
    build_isr_no_err 27
    build_isr_no_err 28
    build_isr_err    29
    build_isr_err    30
    build_isr_no_err 31

    .set i, 32
    .rept 0x80+1
        build_isr_no_err %i
        .set i, i+1
    .endr

    .section .rodata

    .align 8
    .globl ISR_START_ADDR
    .type  ISR_START_ADDR @object
    ISR_START_ADDR:
        .quad ISR0
    ",
    options(att_syntax),
);

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

#[allow(missing_docs)]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExtendedContext {
    /// For FPU states
    data: [u8; 512],
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

/// State of the interrupt flag.
pub struct IrqState(u64);

impl InterruptContext {
    pub fn set_return_value(&mut self, value: u64) {
        // The return value is stored in rax.
        self.rax = value;
    }

    pub fn set_return_address(&mut self, addr: u64, user: bool) {
        // The return address is stored in rip.
        self.rip = addr;
        if user {
            self.cs = 0x2b; // User code segment
        } else {
            self.cs = 0x08; // Kernel code segment
        }
    }

    pub fn set_stack_pointer(&mut self, sp: u64, user: bool) {
        // The stack pointer is stored in rsp.
        self.rsp = sp;
        if user {
            self.ss = 0x33; // User stack segment
        } else {
            self.ss = 0x10; // Kernel stack segment
        }
    }

    pub fn set_interrupt_enabled(&mut self, enabled: bool) {
        // The interrupt state is stored in eflags.
        if enabled {
            self.eflags |= 0x200; // Set the interrupt flag
        } else {
            self.eflags &= !0x200; // Clear the interrupt flag
        }
    }
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
    pub(crate) fn new() -> (Self, usize) {
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

        icr.write(0xc4607);
        while icr.read() & 0x1000 != 0 {
            pause();
        }
    }

    /// Send EOI to APIC so that it can send more interrupts.
    pub fn end_of_interrupt(&self) {
        self.apic_base.end_of_interrupt()
    }
}

impl IrqState {
    pub fn restore(self) {
        let Self(state) = self;

        unsafe {
            asm!(
                "push {state}",
                "popf",
                state = in(reg) state,
                options(att_syntax, nomem)
            );
        }
    }
}

pub fn enable_irqs() {
    unsafe {
        asm!("sti", options(att_syntax, nomem, nostack));
    }
}

pub fn disable_irqs() {
    unsafe {
        asm!("cli", options(att_syntax, nomem, nostack));
    }
}

pub fn disable_irqs_save() -> IrqState {
    let state: u64;
    unsafe {
        asm!(
            "pushf",
            "pop {state}",
            "cli",
            state = out(reg) state,
            options(att_syntax, nomem)
        );
    }

    IrqState(state)
}

extern "C" {
    pub fn _arch_fork_return();
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
