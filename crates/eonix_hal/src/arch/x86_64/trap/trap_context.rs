use core::arch::asm;
use core::mem::offset_of;

use eonix_hal_traits::fault::{Fault, PageFaultErrorCode};
use eonix_hal_traits::trap::{RawTrapContext, TrapType};
use eonix_mm::address::VAddr;

use crate::processor::CPU;

#[repr(C, align(16))]
#[derive(Clone, Copy, Default)]
pub struct TrapContext {
    rcx: u64,
    rdx: u64,
    rdi: u64,
    rsi: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,

    /// on `trap_return`: save capturer %rsp, load %rax
    /// on `trap_entry`: save %rax, load capturer %rsp
    rax: u64,
    rbx: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rbp: u64,

    int_no: u64,
    errcode: u64,
    rip: u64,
    cs: u64,
    flags: u64,
    rsp: u64,
    ss: u64,
}

impl TrapContext {
    pub const OFFSET_RAX: usize = offset_of!(TrapContext, rax);
    pub const OFFSET_RBX: usize = offset_of!(TrapContext, rbx);
    pub const OFFSET_RCX: usize = offset_of!(TrapContext, rcx);
    pub const OFFSET_RDX: usize = offset_of!(TrapContext, rdx);
    pub const OFFSET_RDI: usize = offset_of!(TrapContext, rdi);
    pub const OFFSET_RSI: usize = offset_of!(TrapContext, rsi);
    pub const OFFSET_R8: usize = offset_of!(TrapContext, r8);
    pub const OFFSET_R9: usize = offset_of!(TrapContext, r9);
    pub const OFFSET_R10: usize = offset_of!(TrapContext, r10);
    pub const OFFSET_R11: usize = offset_of!(TrapContext, r11);
    pub const OFFSET_R12: usize = offset_of!(TrapContext, r12);
    pub const OFFSET_R13: usize = offset_of!(TrapContext, r13);
    pub const OFFSET_R14: usize = offset_of!(TrapContext, r14);
    pub const OFFSET_R15: usize = offset_of!(TrapContext, r15);
    pub const OFFSET_RBP: usize = offset_of!(TrapContext, rbp);
    pub const OFFSET_INT_NO: usize = offset_of!(TrapContext, int_no);
    pub const OFFSET_ERRCODE: usize = offset_of!(TrapContext, errcode);
    pub const OFFSET_RIP: usize = offset_of!(TrapContext, rip);
    pub const OFFSET_CS: usize = offset_of!(TrapContext, cs);
    pub const OFFSET_FLAGS: usize = offset_of!(TrapContext, flags);
    pub const OFFSET_RSP: usize = offset_of!(TrapContext, rsp);
    pub const OFFSET_SS: usize = offset_of!(TrapContext, ss);

    fn get_fault_type(&self) -> Fault {
        match self.int_no {
            6 | 8 => Fault::InvalidOp,
            13 => Fault::BadAccess,
            14 => {
                let mut error_code = PageFaultErrorCode::empty();

                if self.errcode & 2 != 0 {
                    error_code |= PageFaultErrorCode::Write;
                } else if self.errcode & 16 != 0 {
                    error_code |= PageFaultErrorCode::InstructionFetch;
                } else {
                    error_code |= PageFaultErrorCode::Read;
                }

                if self.errcode & 4 != 0 {
                    error_code |= PageFaultErrorCode::UserAccess;
                }

                #[inline(always)]
                fn get_page_fault_address() -> VAddr {
                    let cr2: usize;
                    unsafe {
                        asm!(
                            "mov %cr2, {}",
                            out(reg) cr2,
                            options(att_syntax)
                        );
                    }
                    VAddr::from(cr2)
                }

                Fault::PageFault {
                    error_code,
                    address: get_page_fault_address(),
                }
            }
            code @ 0..0x20 => Fault::Unknown(code as usize),
            _ => unreachable!(),
        }
    }
}

impl RawTrapContext for TrapContext {
    type FIrq = impl FnOnce(fn(irqno: usize));
    type FTimer = fn(handler: fn());

    fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    fn trap_type(&self) -> TrapType<Self::FIrq, Self::FTimer> {
        match self.int_no {
            0..0x20 => TrapType::Fault(self.get_fault_type()),
            0x40 => TrapType::Timer {
                callback: |handler| {
                    CPU::local().as_mut().end_of_interrupt();
                    handler();
                },
            },
            0x80 => TrapType::Syscall {
                no: self.rax as usize,
                args: [
                    self.rbx as usize,
                    self.rcx as usize,
                    self.rdx as usize,
                    self.rsi as usize,
                    self.rdi as usize,
                    self.rbp as usize,
                ],
            },
            no => TrapType::Irq {
                callback: move |handler| {
                    let irqno = no as usize - 0x20;

                    use crate::arch::io::Port8;

                    const PIC1_COMMAND: Port8 = Port8::new(0x20);
                    const PIC2_COMMAND: Port8 = Port8::new(0xA0);

                    PIC1_COMMAND.write(0x20); // EOI
                    if irqno >= 8 {
                        PIC2_COMMAND.write(0x20); // EOI
                    }

                    handler(irqno);
                },
            },
        }
    }

    fn get_program_counter(&self) -> usize {
        self.rip as usize
    }

    fn get_stack_pointer(&self) -> usize {
        self.rsp as usize
    }

    fn set_program_counter(&mut self, pc: usize) {
        self.rip = pc as u64
    }

    fn set_stack_pointer(&mut self, sp: usize) {
        self.rsp = sp as u64
    }

    fn is_interrupt_enabled(&self) -> bool {
        self.flags & 0x200 != 0
    }

    fn set_interrupt_enabled(&mut self, enabled: bool) {
        if enabled {
            self.flags |= 0x200;
        } else {
            self.flags &= !0x200;
        }
    }

    fn is_user_mode(&self) -> bool {
        self.cs & 3 == 3
    }

    fn set_user_mode(&mut self, user: bool) {
        self.cs = if user { 0x2b } else { 0x08 };
        self.ss = if user { 0x33 } else { 0x10 };
    }

    fn set_user_return_value(&mut self, retval: usize) {
        self.rax = retval as u64;
    }

    fn set_user_call_frame<E>(
        &mut self, pc: usize, sp: Option<usize>, ra: Option<usize>,
        args: &[usize], write_memory: impl Fn(VAddr, &[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        self.set_program_counter(pc);

        let mut sp = sp.unwrap_or_else(|| self.get_stack_pointer());

        let arg_size = args.len() * 4;

        sp -= arg_size;
        sp &= !0xf; // Align to 16 bytes
        for (idx, arg) in args.iter().enumerate() {
            write_memory(VAddr::from(sp + idx * 8), &arg.to_ne_bytes())?;
        }

        if let Some(ra) = ra {
            sp -= 4; // Space for return address
            write_memory(VAddr::from(sp), &ra.to_ne_bytes())?;
        }

        self.set_stack_pointer(sp);
        Ok(())
    }
}
