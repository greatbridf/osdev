use crate::arch::time::set_next_timer;
use core::arch::asm;
use eonix_hal_traits::{
    fault::{Fault, PageFaultErrorCode},
    trap::{RawTrapContext, TrapType},
};
use eonix_mm::address::VAddr;
use riscv::{
    interrupt::{Exception, Interrupt, Trap},
    register::{
        scause::{self, Scause},
        sstatus::{self, Sstatus, FS, SPP},
        stval,
    },
    ExceptionNumber, InterruptNumber,
};

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Registers {
    ra: u64,
    sp: u64,
    gp: u64,
    tp: u64,
    t1: u64,
    t2: u64,
    t0: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
    a6: u64,
    a7: u64,
    t3: u64,
    t4: u64,
    t5: u64,
    t6: u64,
    s0: u64,
    s1: u64,
    s2: u64,
    s3: u64,
    s4: u64,
    s5: u64,
    s6: u64,
    s7: u64,
    s8: u64,
    s9: u64,
    s10: u64,
    s11: u64,
}

/// Saved CPU context when a trap (interrupt or exception) occurs on RISC-V 64.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TrapContext {
    regs: Registers,

    sstatus: Sstatus,
    sepc: usize,
    scause: Scause,
}

impl Registers {
    pub const OFFSET_RA: usize = 0 * 8;
    pub const OFFSET_SP: usize = 1 * 8;
    pub const OFFSET_GP: usize = 2 * 8;
    pub const OFFSET_TP: usize = 3 * 8;
    pub const OFFSET_T1: usize = 4 * 8;
    pub const OFFSET_T2: usize = 5 * 8;
    pub const OFFSET_T0: usize = 6 * 8;
    pub const OFFSET_A0: usize = 7 * 8;
    pub const OFFSET_A1: usize = 8 * 8;
    pub const OFFSET_A2: usize = 9 * 8;
    pub const OFFSET_A3: usize = 10 * 8;
    pub const OFFSET_A4: usize = 11 * 8;
    pub const OFFSET_A5: usize = 12 * 8;
    pub const OFFSET_A6: usize = 13 * 8;
    pub const OFFSET_A7: usize = 14 * 8;
    pub const OFFSET_T3: usize = 15 * 8;
    pub const OFFSET_T4: usize = 16 * 8;
    pub const OFFSET_T5: usize = 17 * 8;
    pub const OFFSET_T6: usize = 18 * 8;
    pub const OFFSET_S0: usize = 19 * 8;
    pub const OFFSET_S1: usize = 20 * 8;
    pub const OFFSET_S2: usize = 21 * 8;
    pub const OFFSET_S3: usize = 22 * 8;
    pub const OFFSET_S4: usize = 23 * 8;
    pub const OFFSET_S5: usize = 24 * 8;
    pub const OFFSET_S6: usize = 25 * 8;
    pub const OFFSET_S7: usize = 26 * 8;
    pub const OFFSET_S8: usize = 27 * 8;
    pub const OFFSET_S9: usize = 28 * 8;
    pub const OFFSET_S10: usize = 29 * 8;
    pub const OFFSET_S11: usize = 30 * 8;
}

impl TrapContext {
    pub const OFFSET_SSTATUS: usize = 31 * 8;
    pub const OFFSET_SEPC: usize = 32 * 8;
    pub const OFFSET_SCAUSE: usize = 33 * 8;

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
}

impl RawTrapContext for TrapContext {
    type FIrq = fn(handler: fn(irqno: usize));
    type FTimer = fn(handler: fn());

    fn new() -> Self {
        let mut sstatus = Sstatus::from_bits(0);
        sstatus.set_fs(FS::Initial);
        sstatus.set_sum(true);

        Self {
            regs: Registers::default(),
            sstatus,
            sepc: 0,
            scause: Scause::from_bits(0),
        }
    }

    fn trap_type(&self) -> TrapType<Self::FIrq, Self::FTimer> {
        let cause = self.scause.cause();
        match cause {
            Trap::Interrupt(i) => {
                match Interrupt::from_number(i).unwrap() {
                    Interrupt::SupervisorTimer => TrapType::Timer {
                        callback: |handler| {
                            handler();
                            set_next_timer();
                        },
                    },
                    Interrupt::SupervisorExternal => TrapType::Irq {
                        callback: |handler| {
                            // TODO: Read PLIC to determine the IRQ number.
                            //       Use 0xa (serial) for now.
                            handler(0xa);
                        },
                    },
                    // soft interrupt
                    _ => TrapType::Fault(Fault::Unknown(i)),
                }
            }
            Trap::Exception(e) => {
                match Exception::from_number(e).unwrap() {
                    Exception::InstructionMisaligned
                    | Exception::LoadMisaligned
                    | Exception::InstructionFault
                    | Exception::LoadFault
                    | Exception::StoreFault
                    | Exception::StoreMisaligned => TrapType::Fault(Fault::BadAccess),
                    Exception::IllegalInstruction => TrapType::Fault(Fault::InvalidOp),
                    Exception::UserEnvCall => TrapType::Syscall {
                        no: self.syscall_no(),
                        args: self.syscall_args(),
                    },
                    exception @ (Exception::InstructionPageFault
                    | Exception::LoadPageFault
                    | Exception::StorePageFault) => {
                        #[inline(always)]
                        fn get_page_fault_address() -> VAddr {
                            VAddr::from(stval::read())
                        }
                        TrapType::Fault(Fault::PageFault {
                            error_code: self.get_page_fault_error_code(exception),
                            address: get_page_fault_address(),
                        })
                    }
                    // breakpoint and supervisor env call
                    _ => TrapType::Fault(Fault::Unknown(e)),
                }
            }
        }
    }

    fn get_program_counter(&self) -> usize {
        self.sepc
    }

    fn get_stack_pointer(&self) -> usize {
        self.regs.sp as usize
    }

    fn set_program_counter(&mut self, pc: usize) {
        self.sepc = pc;
    }

    fn set_stack_pointer(&mut self, sp: usize) {
        self.regs.sp = sp as u64;
    }

    fn is_interrupt_enabled(&self) -> bool {
        self.sstatus.spie()
    }

    fn set_interrupt_enabled(&mut self, enabled: bool) {
        self.sstatus.set_spie(enabled);
    }

    fn is_user_mode(&self) -> bool {
        self.sstatus.spp() == SPP::User
    }

    fn set_user_mode(&mut self, user: bool) {
        match user {
            true => self.sstatus.set_spp(SPP::User),
            false => self.sstatus.set_spp(SPP::Supervisor),
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
    /// TODO: get PageFaultErrorCode also need check pagetable
    fn get_page_fault_error_code(&self, exception: Exception) -> PageFaultErrorCode {
        let mut error_code = PageFaultErrorCode::empty();

        match exception {
            Exception::InstructionPageFault => {
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

        if self.sstatus.spp() == SPP::User {
            error_code |= PageFaultErrorCode::UserAccess;
        }

        error_code
    }
}
