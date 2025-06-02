use core::arch::asm;
use riscv::{
    interrupt::{Exception, Interrupt, Trap}, register::{
        scause, sie, sstatus::{self, Sstatus, SPP}, stval
    }, ExceptionNumber, InterruptNumber
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
    pub sepc: usize,    // sepc (Supervisor Exception Program Counter). Program counter at trap.
    
    pub kernel_sp: usize,
    pub kernel_ra: usize,
    pub kernel_s: [usize; 12],
    pub kernel_fp: usize,
    pub kernel_tp: usize,

    // may need to save
    // FPU
    // pub fpu_regs: FpuRegisters,
}

impl FpuRegisters {
    pub fn new() -> Self {
        unsafe { core::mem::zeroed() }
    }

    /// Save reg -> mem
    pub fn save(&mut self) {
        unsafe {
            let base_ptr: *mut u64 = self.f.as_mut_ptr();
            let fcsr_ptr: *mut u32 = &mut self.fcsr;
            let mut _fcsr_val: u32 = 0;
            asm!(
            "fsd f0,  (0 * 8)({base})",
            "fsd f1,  (1 * 8)({base})",
            "fsd f2,  (2 * 8)({base})",
            "fsd f3,  (3 * 8)({base})",
            "fsd f4,  (4 * 8)({base})",
            "fsd f5,  (5 * 8)({base})",
            "fsd f6,  (6 * 8)({base})",
            "fsd f7,  (7 * 8)({base})",
            "fsd f8,  (8 * 8)({base})",
            "fsd f9,  (9 * 8)({base})",
            "fsd f10, (10 * 8)({base})",
            "fsd f11, (11 * 8)({base})",
            "fsd f12, (12 * 8)({base})",
            "fsd f13, (13 * 8)({base})",
            "fsd f14, (14 * 8)({base})",
            "fsd f15, (15 * 8)({base})",
            "fsd f16, (16 * 8)({base})",
            "fsd f17, (17 * 8)({base})",
            "fsd f18, (18 * 8)({base})",
            "fsd f19, (19 * 8)({base})",
            "fsd f20, (20 * 8)({base})",
            "fsd f21, (21 * 8)({base})",
            "fsd f22, (22 * 8)({base})",
            "fsd f23, (23 * 8)({base})",
            "fsd f24, (24 * 8)({base})",
            "fsd f25, (25 * 8)({base})",
            "fsd f26, (26 * 8)({base})",
            "fsd f27, (27 * 8)({base})",
            "fsd f28, (28 * 8)({base})",
            "fsd f29, (29 * 8)({base})",
            "fsd f30, (30 * 8)({base})",
            "fsd f31, (31 * 8)({base})",
            "csrr {fcsr_val}, fcsr", // Read fcsr into fcsr_val (which is in a general-purpose register)
            "sw {fcsr_val}, 0({fcsr_ptr})",
            base = in(reg) base_ptr,
            fcsr_val = out(reg) _fcsr_val,
            fcsr_ptr = in(reg) fcsr_ptr,
            options(nostack, nomem, preserves_flags));
        }
    }

    pub fn restore(&mut self) {
        let base_ptr: *const u64 = self.f.as_ptr();
        let fcsr_ptr: *const u32 = &self.fcsr;
        let mut _fcsr_val: u64;

        unsafe {
            asm!(
            "fld f0,  (0 * 8)({base})",
            "fld f1,  (1 * 8)({base})",
            "fld f2,  (2 * 8)({base})",
            "fld f3,  (3 * 8)({base})",
            "fld f4,  (4 * 8)({base})",
            "fld f5,  (5 * 8)({base})",
            "fld f6,  (6 * 8)({base})",
            "fld f7,  (7 * 8)({base})",
            "fld f8,  (8 * 8)({base})",
            "fld f9,  (9 * 8)({base})",
            "fld f10, (10 * 8)({base})",
            "fld f11, (11 * 8)({base})",
            "fld f12, (12 * 8)({base})",
            "fld f13, (13 * 8)({base})",
            "fld f14, (14 * 8)({base})",
            "fld f15, (15 * 8)({base})",
            "fld f16, (16 * 8)({base})",
            "fld f17, (17 * 8)({base})",
            "fld f18, (18 * 8)({base})",
            "fld f19, (19 * 8)({base})",
            "fld f20, (20 * 8)({base})",
            "fld f21, (21 * 8)({base})",
            "fld f22, (22 * 8)({base})",
            "fld f23, (23 * 8)({base})",
            "fld f24, (24 * 8)({base})",
            "fld f25, (25 * 8)({base})",
            "fld f26, (26 * 8)({base})",
            "fld f27, (27 * 8)({base})",
            "fld f28, (28 * 8)({base})",
            "fld f29, (29 * 8)({base})",
            "fld f30, (30 * 8)({base})",
            "fld f31, (31 * 8)({base})",
            "lw {fcsr_val}, 0({fcsr_ptr})", // Load from memory (fcsr_ptr)
            "csrw fcsr, {fcsr_val}",
            base = in(reg) base_ptr,
            fcsr_val = out(reg) _fcsr_val,
            fcsr_ptr = in(reg) fcsr_ptr,
            options(nostack, nomem, preserves_flags));
        }
    }
}


/// TODO: will be displaced after origin's branch be mergered.
use bitflags::bitflags;
bitflags! {
    #[derive(Debug)]
    pub struct PageFaultErrorCode: u32 {
        const NonPresent = 1;
        const Read = 2;
        const Write = 4;
        const InstructionFetch = 8;
        const UserAccess = 16;
    }
}

#[derive(Debug)]
pub enum Fault {
    InvalidOp,
    BadAccess,
    PageFault(PageFaultErrorCode),
    Unknown(usize),
}

pub enum TrapType {
    Syscall { no: usize, args: [usize; 6] },
    Fault(Fault),
    Irq(usize),
    Timer,
}

impl TrapContext {
    /// TODO: temporarily all zero, may change in future
    pub fn new() -> Self {
        Self {
            x: [0; 32],
            sstatus: sstatus::read(),
            sepc: 0,
            kernel_sp: 0,
            kernel_ra: 0,
            kernel_s: [0; 12],
            kernel_fp: 0,
            kernel_tp: 0
        }
    }

    fn syscall_no(&self) -> usize {
        self.x[17]
    }

    fn syscall_args(&self) -> [usize; 6] {
        [
            self.x[10],
            self.x[11],
            self.x[12],
            self.x[13],
            self.x[14],
            self.x[15],
        ]
    }

    pub fn trap_type(&self) -> TrapType {
        let scause = scause::read();
        let cause = scause.cause();
        match cause {
            Trap::Interrupt(i) => {
                match Interrupt::from_number(i).unwrap() {
                    Interrupt::SupervisorTimer => TrapType::Timer,
                    Interrupt::SupervisorExternal => TrapType::Irq(0),
                    // soft interrupt
                    _ => TrapType::Fault(Fault::Unknown(i)),
                }
            }
            Trap::Exception(e) => {
                match Exception::from_number(e).unwrap() {
                    Exception::InstructionMisaligned |
                    Exception::LoadMisaligned |
                    Exception::InstructionFault |
                    Exception::LoadFault |
                    Exception::StoreFault |
                    Exception::StoreMisaligned => {
                        TrapType::Fault(Fault::BadAccess)
                    },
                    Exception::IllegalInstruction => {
                        TrapType::Fault(Fault::InvalidOp)
                    }
                    Exception::UserEnvCall => {
                        TrapType::Syscall { 
                            no: self.syscall_no(),
                            args: self.syscall_args()
                        }
                    },
                    Exception::InstructionPageFault |
                    Exception::LoadPageFault |
                    Exception::StorePageFault => {
                        let e = Exception::from_number(e).unwrap();
                        TrapType::Fault(Fault::PageFault(get_page_fault_error_code(e)))
                    },
                    // breakpoint and supervisor env call
                    _ => TrapType::Fault(Fault::Unknown(e)),
                }
            },
        }
    }

    pub fn get_program_counter(&self) -> usize {
        self.sepc
    }

    pub fn get_stack_pointer(&self) -> usize {
        self.x[2]
    }

    pub fn set_program_counter(&mut self, pc: usize) {
        self.sepc = pc;
    }

    pub fn set_stack_pointer(&mut self, sp: usize) {
        self.x[2] = sp;
    }

    pub fn is_interrupt_enabled(&self) -> bool {
        self.sstatus.sie()
    }

    /// TODO: may need more precise control
    pub fn set_interrupt_enabled(&mut self, enabled: bool) {
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

    pub fn is_user_mode(&self) -> bool {
        self.sstatus.spp() == SPP::User
    }

    pub fn set_user_mode(&mut self, user: bool) {
        match user {
            true => self.sstatus.set_spp(SPP::User),
            false => self.sstatus.set_spp(SPP::Supervisor),
        }
    }

    pub fn set_user_return_value(&mut self, retval: usize) {
        self.sepc = retval;
    }
}

/// TODO: get PageFaultErrorCode also need check pagetable
fn get_page_fault_error_code(exception_type: Exception) -> PageFaultErrorCode {
    let scause_val = stval::read();
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
