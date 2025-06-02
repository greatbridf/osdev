use riscv::register::sstatus::{Sstatus, SPP};


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

impl TrapContext {
    pub fn set_return_value(&mut self, value: usize) {
        // a0, x10
        self.x[10] = value;
    }

    pub fn set_return_address(&mut self, addr: usize, user: bool) {
        self.sepc = addr; // set Supervisor Exception Program Counter

        // if user==true,set SPP to U-mode (0)
        // if user==false, set SPP to S-mode (1)
        if user {
            self.sstatus.set_spp(SPP::User);
        } else {
            self.sstatus.set_spp(SPP::Supervisor);
        }
    }

    pub fn set_stack_pointer(&mut self, sp: usize, _user: bool) {
        self.x[2] = sp;
    }

    pub fn set_interrupt_enabled(&mut self, enabled: bool) {
        // S mode Previous Interrupt Enable (SPIE)
        if enabled {
            self.sstatus.set_spie(true);
        } else {
            self.sstatus.set_spie(false);
        }
    }
}
