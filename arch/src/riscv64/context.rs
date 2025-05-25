use core::arch::naked_asm;

#[repr(C)]
#[derive(Debug, Default)]
pub struct TaskContext {
    // s0-11
    s: [u64; 12],
    sp: u64,
    ra: u64,
    sstatus: u64,
}

impl TaskContext {
    pub const fn new() -> Self {
        Self {
            s: [0; 12],
            sp: 0,
            ra: 0,
            sstatus: 0,
        }
    }

    pub fn ip(&mut self, ip: usize) {
        self.ra = ip as u64;
    }

    pub fn entry_point(&mut self, entry: usize) {
        self.ra = entry as u64;
    }

    pub fn sp(&mut self, sp: usize) {
        self.sp = sp as u64;
    }

    pub fn sstatus(&mut self, status: usize) {
        self.sstatus = status as u64;
    }

    pub fn call1(&mut self, func: unsafe extern "C" fn(usize) -> !, arg: [usize; 1]) {
        self.entry_point(Self::do_call as usize);
        self.s[0] = func as u64;
        self.s[1] = arg[0] as u64;
    }

    pub fn call2(&mut self, func: unsafe extern "C" fn(usize, usize) -> !, arg: [usize; 2]) {
        self.entry_point(Self::do_call as usize);
        self.s[0] = func as u64;
        self.s[1] = arg[0] as u64;
        self.s[2] = arg[1] as u64;
    }

    pub fn call3(&mut self, func: unsafe extern "C" fn(usize, usize, usize) -> !, arg: [usize; 3]) {
        self.entry_point(Self::do_call as usize);
        self.s[0] = func as u64;
        self.s[1] = arg[0] as u64;
        self.s[2] = arg[1] as u64;
        self.s[3] = arg[2] as u64;
    }

    pub fn call4(
        &mut self,
        func: unsafe extern "C" fn(usize, usize, usize, usize) -> !,
        arg: [usize; 4],
    ) {
        self.entry_point(Self::do_call as usize);
        self.s[0] = func as u64;
        self.s[1] = arg[0] as u64;
        self.s[2] = arg[1] as u64;
        self.s[3] = arg[2] as u64;
        self.s[4] = arg[3] as u64;
    }

    pub fn call5(
        &mut self,
        func: unsafe extern "C" fn(usize, usize, usize, usize, usize) -> !,
        arg: [usize; 5],
    ) {
        self.entry_point(Self::do_call as usize);
        self.s[0] = func as u64;
        self.s[1] = arg[0] as u64;
        self.s[2] = arg[1] as u64;
        self.s[3] = arg[2] as u64;
        self.s[4] = arg[3] as u64;
    }

    pub fn interrupt(&mut self, is_enabled: bool) {
        // sstatus: SIE bit is bit 1
        const SSTATUS_SIE_BIT: u64 = 1 << 1; // 0x2

        if is_enabled {
            self.sstatus |= SSTATUS_SIE_BIT;
        } else {
            self.sstatus &= !SSTATUS_SIE_BIT;
        }
    }

    /// SPIE bit
    pub fn interrupt_on_return(&mut self, will_enable: bool) {
        const SSTATUS_SPIE_BIT: u64 = 1 << 5;
        if will_enable {
            self.sstatus |= SSTATUS_SPIE_BIT;
        } else {
            self.sstatus &= !SSTATUS_SPIE_BIT;
        }
    }

    #[naked]
    #[no_mangle]
    pub unsafe extern "C" fn __task_context_switch(from: *mut Self, to: *const Self) {
        // Input arguments `from` and `to` will be in `a0` (x10) and `a1` (x11).
        naked_asm!(
            // Save current task's callee-saved registers to `from` context
            "sd   s0, 0(a0)",
            "sd   s1, 8(a0)",
            "sd   s2, 16(a0)",
            "sd   s3, 24(a0)",
            "sd   s4, 32(a0)",
            "sd   s5, 40(a0)",
            "sd   s6, 48(a0)",
            "sd   s7, 56(a0)",
            "sd   s8, 64(a0)",
            "sd   s9, 72(a0)",
            "sd  s10, 80(a0)",
            "sd  s11, 88(a0)",
            "sd   sp, 96(a0)",
            "sd   ra, 104(a0)",
            "csrr s0, sstatus",
            "sd   s0, 112(a0)", // Store sstatus at offset 112

            // Load next task's callee-saved registers from `to` context
            "ld   s0, 0(a1)",
            "ld   s1, 8(a1)",
            "ld   s2, 16(a1)",
            "ld   s3, 24(a1)",
            "ld   s4, 32(a1)",
            "ld   s5, 40(a1)",
            "ld   s6, 48(a1)",
            "ld   s7, 56(a1)",
            "ld   s8, 64(a1)",
            "ld   s9, 72(a1)",
            "ld  s10, 80(a1)",
            "ld  s11, 88(a1)",
            "ld   sp, 96(a1)",
            "ld   ra, 104(a1)",
            "ld   t0, 112(a1)",
            "csrw sstatus, t0",

            "ret",
        );
    }

    pub fn switch(from: &mut Self, to: &mut Self) {
        unsafe {
            TaskContext::__task_context_switch(from, to);
        }
    }

    #[naked]
    #[no_mangle]
    /// Maximum of 5 arguments supported.
    unsafe extern "C" fn do_call() -> ! {
        naked_asm!(
            // Move function pointer.
            "mv   t0, s0",

            // Move arguments.
            "mv   a0, s1",
            "mv   a1, s2",
            "mv   a2, s3",
            "mv   a3, s4",
            "mv   a4, s5",

            "mv   s0, zero", // Set s0 (fp) to 0.

            "jr   t0", // Jump to t0.
        );
    }
}
