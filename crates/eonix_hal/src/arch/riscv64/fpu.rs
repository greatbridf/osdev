use core::arch::asm;
use eonix_hal_traits::fpu::RawFpuState;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct FpuState {
    pub f: [u64; 32],
    pub fcsr: u32,
}

impl RawFpuState for FpuState {
    fn new() -> Self {
        unsafe { core::mem::zeroed() }
    }

    /// Save reg -> mem
    fn save(&mut self) {
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

    fn restore(&mut self) {
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
            options(nostack, preserves_flags));
        }
    }
}
