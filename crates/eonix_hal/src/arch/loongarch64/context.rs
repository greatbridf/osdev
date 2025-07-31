use core::arch::naked_asm;
use eonix_hal_traits::context::RawTaskContext;

#[repr(C)]
#[derive(Debug)]
pub struct TaskContext {
    sx: [u64; 9],
    sp: u64,
    ra: u64,
    fp: u64,
    crmd: usize,
}

impl RawTaskContext for TaskContext {
    fn new() -> Self {
        Self::new()
    }

    fn set_program_counter(&mut self, pc: usize) {
        self.ra = pc as u64;
    }

    fn set_stack_pointer(&mut self, sp: usize) {
        self.sp = sp as u64;
    }

    fn is_interrupt_enabled(&self) -> bool {
        self.crmd & (1 << 2) != 0
    }

    fn set_interrupt_enabled(&mut self, is_enabled: bool) {
        if is_enabled {
            self.crmd = self.crmd | (1 << 2);
        } else {
            self.crmd = self.crmd & !(1 << 2);
        }
    }

    fn call(&mut self, func: unsafe extern "C" fn(usize) -> !, arg: usize) {
        self.sx[0] = func as u64;
        self.sx[1] = arg as u64;

        self.set_program_counter(Self::do_call as usize);
    }

    #[unsafe(naked)]
    unsafe extern "C" fn switch(from: &mut Self, to: &mut Self) {
        // Input arguments `from` and `to` will be in `a0` (x10) and `a1` (x11).
        naked_asm!(
            // Save current task's callee-saved registers to `from` context
            "st.d  $s0, $a0,  0",
            "st.d  $s1, $a0,  8",
            "st.d  $s2, $a0, 16",
            "st.d  $s3, $a0, 24",
            "st.d  $s4, $a0, 32",
            "st.d  $s5, $a0, 40",
            "st.d  $s6, $a0, 48",
            "st.d  $s7, $a0, 56",
            "st.d  $s8, $a0, 64",
            "st.d  $sp, $a0, 72",
            "st.d  $ra, $a0, 80",
            "st.d  $fp, $a0, 88",
            "csrrd $t0, 0", // CRMD
            "st.d  $t0, $a0, 96",
            "",
            "ld.d  $s0, $a1,  0",
            "ld.d  $s1, $a1,  8",
            "ld.d  $s2, $a1, 16",
            "ld.d  $s3, $a1, 24",
            "ld.d  $s4, $a1, 32",
            "ld.d  $s5, $a1, 40",
            "ld.d  $s6, $a1, 48",
            "ld.d  $s7, $a1, 56",
            "ld.d  $s8, $a1, 64",
            "ld.d  $sp, $a1, 72",
            "ld.d  $ra, $a1, 80",
            "ld.d  $fp, $a1, 88",
            "ld.d  $t0, $a1, 96",
            "csrwr $t0, 0", // CRMD
            "ret",
        );
    }
}

impl TaskContext {
    pub const fn new() -> Self {
        Self {
            sx: [0; 9],
            sp: 0,
            ra: 0,
            fp: 0,
            crmd: 1 << 4, // PG = 1
        }
    }

    #[unsafe(naked)]
    /// Maximum of 5 arguments supported.
    unsafe extern "C" fn do_call() -> ! {
        naked_asm!(
            "move $a0, $s1", // Args
            "move $a1, $s2",
            "move $a2, $s3",
            "move $a3, $s4",
            "move $a4, $s5",
            "move $fp, $zero", // Set frame pointer to 0.
            "move $ra, $zero",
            "jirl $zero, $s0, 0",
        );
    }
}
