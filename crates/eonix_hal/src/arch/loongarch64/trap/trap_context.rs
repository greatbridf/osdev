use crate::processor::CPU;
use core::{arch::asm, mem::offset_of};
use eonix_hal_traits::{
    fault::{Fault, PageFaultErrorCode},
    trap::{RawTrapContext, TrapType},
};
use eonix_mm::address::VAddr;
use loongArch64::register::{
    badv,
    estat::{Estat, Exception, Interrupt, Trap},
    ticlr,
};

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Registers {
    ra: u64,
    tp: u64,
    sp: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
    a6: u64,
    a7: u64,
    t0: u64,
    t1: u64,
    t2: u64,
    t3: u64,
    t4: u64,
    t5: u64,
    t6: u64,
    t7: u64,
    t8: u64,
    u0: u64,
    fp: u64,
    s0: u64,
    s1: u64,
    s2: u64,
    s3: u64,
    s4: u64,
    s5: u64,
    s6: u64,
    s7: u64,
    s8: u64,
}

/// Saved CPU context when a trap (interrupt or exception) occurs on RISC-V 64.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TrapContext {
    regs: Registers,
    estat: Estat,
    prmd: usize,
    era: usize,
}

impl Registers {
    pub const OFFSET_RA: usize = offset_of!(Registers, ra);
    pub const OFFSET_TP: usize = offset_of!(Registers, tp);
    pub const OFFSET_SP: usize = offset_of!(Registers, sp);
    pub const OFFSET_A0: usize = offset_of!(Registers, a0);
    pub const OFFSET_A1: usize = offset_of!(Registers, a1);
    pub const OFFSET_A2: usize = offset_of!(Registers, a2);
    pub const OFFSET_A3: usize = offset_of!(Registers, a3);
    pub const OFFSET_A4: usize = offset_of!(Registers, a4);
    pub const OFFSET_A5: usize = offset_of!(Registers, a5);
    pub const OFFSET_A6: usize = offset_of!(Registers, a6);
    pub const OFFSET_A7: usize = offset_of!(Registers, a7);
    pub const OFFSET_T0: usize = offset_of!(Registers, t0);
    pub const OFFSET_T1: usize = offset_of!(Registers, t1);
    pub const OFFSET_T2: usize = offset_of!(Registers, t2);
    pub const OFFSET_T3: usize = offset_of!(Registers, t3);
    pub const OFFSET_T4: usize = offset_of!(Registers, t4);
    pub const OFFSET_T5: usize = offset_of!(Registers, t5);
    pub const OFFSET_T6: usize = offset_of!(Registers, t6);
    pub const OFFSET_T7: usize = offset_of!(Registers, t7);
    pub const OFFSET_T8: usize = offset_of!(Registers, t8);
    pub const OFFSET_U0: usize = offset_of!(Registers, u0);
    pub const OFFSET_FP: usize = offset_of!(Registers, fp);
    pub const OFFSET_S0: usize = offset_of!(Registers, s0);
    pub const OFFSET_S1: usize = offset_of!(Registers, s1);
    pub const OFFSET_S2: usize = offset_of!(Registers, s2);
    pub const OFFSET_S3: usize = offset_of!(Registers, s3);
    pub const OFFSET_S4: usize = offset_of!(Registers, s4);
    pub const OFFSET_S5: usize = offset_of!(Registers, s5);
    pub const OFFSET_S6: usize = offset_of!(Registers, s6);
    pub const OFFSET_S7: usize = offset_of!(Registers, s7);
    pub const OFFSET_S8: usize = offset_of!(Registers, s8);
}

type FIrq = fn(handler: fn(irqno: usize));
type FTimer = fn(handler: fn());

impl TrapContext {
    pub const OFFSET_ESTAT: usize = offset_of!(TrapContext, estat);
    pub const OFFSET_PRMD: usize = offset_of!(TrapContext, prmd);
    pub const OFFSET_ERA: usize = offset_of!(TrapContext, era);

    fn syscall_no(&self) -> usize {
        self.regs.a7 as usize
    }

    fn syscall_args(&self) -> [usize; 6] {
        [
            self.regs.a0 as usize,
            self.regs.a1 as usize,
            self.regs.a2 as usize,
            self.regs.a3 as usize,
            self.regs.a4 as usize,
            self.regs.a5 as usize,
        ]
    }

    fn gen_page_fault(&self, mut err_code: PageFaultErrorCode) -> TrapType<FIrq, FTimer> {
        #[inline(always)]
        fn get_page_fault_address() -> VAddr {
            VAddr::from(badv::read().vaddr())
        }

        err_code.set(PageFaultErrorCode::UserAccess, self.is_user_mode());

        TrapType::Fault(Fault::PageFault {
            error_code: err_code,
            address: get_page_fault_address(),
        })
    }
}

impl RawTrapContext for TrapContext {
    type FIrq = FIrq;
    type FTimer = FTimer;

    fn new() -> Self {
        Self {
            regs: Registers::default(),
            estat: Estat::from(0),
            prmd: 0,
            era: 0,
        }
    }

    fn trap_type(&self) -> TrapType<Self::FIrq, Self::FTimer> {
        match self.estat.cause() {
            Trap::Interrupt(Interrupt::Timer) => TrapType::Timer {
                callback: |handler| {
                    ticlr::clear_timer_interrupt();
                    handler();
                },
            },
            Trap::Interrupt(interrupt) => match interrupt as usize {
                2..=7 => TrapType::Irq {
                    callback: |handler| {
                        todo!("handle IRQs");
                        // let mut cpu = CPU::local();
                        // match cpu.as_mut().interrupt.plic.claim_interrupt() {
                        //     None => {}
                        //     Some(irqno) => {
                        //         cpu.interrupt.plic.complete_interrupt(irqno);
                        //         handler(irqno);
                        //     }
                        // }
                    },
                },
                interrupt => TrapType::Fault(Fault::Unknown(interrupt)),
            },
            Trap::Exception(
                Exception::InstructionPrivilegeIllegal
                | Exception::FetchInstructionAddressError
                | Exception::AddressNotAligned
                | Exception::MemoryAccessAddressError
                | Exception::PagePrivilegeIllegal,
            ) => TrapType::Fault(Fault::BadAccess),
            Trap::Exception(Exception::InstructionNotExist) => TrapType::Fault(Fault::InvalidOp),
            Trap::Exception(Exception::Syscall) => TrapType::Syscall {
                no: self.syscall_no(),
                args: self.syscall_args(),
            },
            Trap::Exception(Exception::LoadPageFault | Exception::PageNonReadableFault) => {
                self.gen_page_fault(PageFaultErrorCode::Read)
            }
            Trap::Exception(Exception::StorePageFault | Exception::PageModifyFault) => {
                self.gen_page_fault(PageFaultErrorCode::Write)
            }
            Trap::Exception(Exception::FetchPageFault | Exception::PageNonExecutableFault) => {
                self.gen_page_fault(PageFaultErrorCode::InstructionFetch)
            }
            Trap::Exception(exception) => TrapType::Fault(Fault::Unknown(exception as usize)),
            Trap::MachineError(_) | Trap::Unknown => todo!(),
        }
    }

    fn get_program_counter(&self) -> usize {
        self.era
    }

    fn get_stack_pointer(&self) -> usize {
        self.regs.sp as usize
    }

    fn set_program_counter(&mut self, pc: usize) {
        self.era = pc;
    }

    fn set_stack_pointer(&mut self, sp: usize) {
        self.regs.sp = sp as u64;
    }

    fn is_interrupt_enabled(&self) -> bool {
        self.prmd & (1 << 2) != 0
    }

    fn set_interrupt_enabled(&mut self, enabled: bool) {
        match enabled {
            true => self.prmd |= 1 << 2,
            false => self.prmd &= !(1 << 2),
        }
    }

    fn is_user_mode(&self) -> bool {
        self.prmd & 0x3 != 0
    }

    fn set_user_mode(&mut self, user: bool) {
        match user {
            true => self.prmd |= 0x3,
            false => self.prmd &= !0x3,
        }
    }

    fn set_user_return_value(&mut self, retval: usize) {
        self.regs.a0 = retval as u64;
    }

    fn set_user_call_frame<E>(
        &mut self,
        pc: usize,
        sp: Option<usize>,
        ra: Option<usize>,
        args: &[usize],
        _write_memory: impl Fn(VAddr, &[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        self.set_program_counter(pc);

        if let Some(sp) = sp {
            self.set_stack_pointer(sp);
        }

        if let Some(ra) = ra {
            self.regs.ra = ra as u64;
        }

        let arg_regs = [
            &mut self.regs.a0,
            &mut self.regs.a1,
            &mut self.regs.a2,
            &mut self.regs.a3,
            &mut self.regs.a4,
            &mut self.regs.a5,
        ];

        for (&arg, reg) in args.iter().zip(arg_regs.into_iter()) {
            *reg = arg as u64;
        }

        Ok(())
    }
}

impl TrapContext {
    fn get_page_fault_error_code(&self, exception: Exception) -> PageFaultErrorCode {
        let mut error_code = PageFaultErrorCode::empty();

        match exception {
            Exception::FetchPageFault => {
                error_code |= PageFaultErrorCode::InstructionFetch;
            }
            Exception::LoadPageFault => {
                error_code |= PageFaultErrorCode::Read;
            }
            Exception::StorePageFault => {
                error_code |= PageFaultErrorCode::Write;
            }
            _ => {
                unreachable!();
            }
        }

        if self.is_user_mode() {
            error_code |= PageFaultErrorCode::UserAccess;
        }

        error_code
    }
}
