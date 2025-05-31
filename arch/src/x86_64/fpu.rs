use core::arch::asm;
use eonix_hal_traits::fpu::RawFpuState;

#[derive(Clone, Copy)]
#[repr(align(16))]
pub struct FpuState([u8; 512]);

impl RawFpuState for FpuState {
    fn new() -> Self {
        Self([0; 512])
    }

    fn save(&mut self) {
        unsafe {
            asm!(
                "fxsave ({0})",
                in(reg) &mut self.0,
                options(att_syntax, nostack, preserves_flags)
            )
        }
    }

    fn restore(&mut self) {
        unsafe {
            asm!(
                "fxrstor ({0})",
                in(reg) &mut self.0,
                options(att_syntax, nostack, preserves_flags)
            )
        }
    }
}
