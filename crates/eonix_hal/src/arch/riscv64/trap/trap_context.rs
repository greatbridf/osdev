use core::arch::asm;
use eonix_hal_traits::{
    fault::{Fault, PageFaultErrorCode},
    trap::{RawTrapContext, TrapType},
};
use riscv::{
    interrupt::{Exception, Interrupt, Trap},
    register::{
        scause::{self, Scause},
        sie,
        sstatus::{self, Sstatus, SPP},
        stval,
    },
    ExceptionNumber, InterruptNumber,
};

/// Floating-point registers context.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct FpuRegisters {
    pub f: [u64; 32],
    pub fcsr: u32,
}

/// Saved CPU context when a trap (interrupt or exception) occurs on RISC-V 64.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapContext {
    pub x: [usize; 32],

    // CSRs
    pub sstatus: Sstatus, // sstatus CSR value. Contains privilege mode, interrupt enable, FPU state.
    pub sepc: usize,      // sepc (Supervisor Exception Program Counter). Program counter at trap.
    pub scause: Scause,   // S-mode Trap Cause Register
}

impl TrapContext {
    fn syscall_no(&self) -> usize {
        self.x[17]
    }

    fn syscall_args(&self) -> [usize; 6] {
        [
            self.x[10], self.x[11], self.x[12], self.x[13], self.x[14], self.x[15],
        ]
    }
}

impl RawTrapContext for TrapContext {
    /// TODO: temporarily all zero, may change in future
    fn new() -> Self {
        Self {
            x: [0; 32],
            sstatus: sstatus::read(),
            sepc: 0,
            scause: Scause::from_bits(0),
        }
    }

    fn trap_type(&self) -> TrapType {
        let cause = self.scause.cause();
        match cause {
            Trap::Interrupt(i) => {
                match Interrupt::from_number(i).unwrap() {
                    Interrupt::SupervisorTimer => TrapType::Timer,
                    // TODO: need to read plic
                    Interrupt::SupervisorExternal => TrapType::Irq(0),
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
                    Exception::InstructionPageFault
                    | Exception::LoadPageFault
                    | Exception::StorePageFault => {
                        let e = Exception::from_number(e).unwrap();
                        TrapType::Fault(Fault::PageFault(self.get_page_fault_error_code(e)))
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
        self.x[2]
    }

    fn set_program_counter(&mut self, pc: usize) {
        self.sepc = pc;
    }

    fn set_stack_pointer(&mut self, sp: usize) {
        self.x[2] = sp;
    }

    fn is_interrupt_enabled(&self) -> bool {
        self.sstatus.sie()
    }

    /// TODO: may need more precise control
    fn set_interrupt_enabled(&mut self, enabled: bool) {
        if enabled {
            self.sstatus.set_sie(enabled);
            unsafe {
                sie::set_sext();
                sie::set_ssoft();
                sie::set_stimer();
            };
        } else {
            self.sstatus.set_sie(enabled);
            unsafe {
                sie::clear_sext();
                sie::clear_ssoft();
                sie::clear_stimer();
            };
        }
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
        self.sepc = retval;
    }
}

impl TrapContext {
    /// TODO: get PageFaultErrorCode also need check pagetable
    fn get_page_fault_error_code(&self, exception_type: Exception) -> PageFaultErrorCode {
        let scause_val = self.scause;
        let mut error_code = PageFaultErrorCode::empty();

        match exception_type {
            Exception::InstructionPageFault => {
                error_code |= PageFaultErrorCode::InstructionFetch;
                error_code |= PageFaultErrorCode::Read;
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
        // TODO: here need check pagetable to confirm NonPresent and UserAccess
        error_code
    }
}
