use core::arch::asm;
use eonix_hal_traits::{
    fault::{Fault, PageFaultErrorCode},
    trap::{RawTrapContext, TrapType},
};
use eonix_mm::address::VAddr;

#[derive(Clone, Copy, Default)]
#[repr(C, align(16))]
pub struct TrapContext {
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rdi: u64,
    rsi: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
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
    fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    fn trap_type(&self) -> TrapType {
        match self.int_no {
            0..0x20 => TrapType::Fault(self.get_fault_type()),
            0x40 => TrapType::Timer,
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
            no => TrapType::Irq(no as usize - 0x20),
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
}
