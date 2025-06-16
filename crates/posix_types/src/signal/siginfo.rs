#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct SigInfo {
    pub si_signo: u32,       // Signal number
    pub si_errno: u32,       // Error number
    pub si_code: u32,        // Signal code
    pub si_trapno: u32,      // Trap number that caused the signal (unused)
    pub si_pid: u32,         // Sending process ID
    pub si_uid: u32,         // Sending user ID
    pub si_status: u32,      // Exit status or signal
    pub si_utime: u64,       // User time consumed
    pub si_stime: u64,       // System time consumed
    pub si_value: u64,       // Signal value (union sigval)
    pub si_int: u32,         // Integer value
    pub si_ptr: usize,       // Pointer value
    pub si_overrun: u32,     // Timer overrun count
    pub si_timerid: u32,     // Timer ID (POSIX.1b timers)
    pub si_addr: usize,      // Address that caused the fault
    pub si_band: u64,        // Band event for SIGPOLL
    pub si_fd: u32,          // File descriptor
    pub si_addr_lsb: u16,    // Least significant bit of address
    pub si_lower: usize,     // Lower bound when address violation occurred
    pub si_upper: usize,     // Upper bound when address violation occurred
    pub si_pkey: u32,        // Protection key on PTE that caused fault
    pub si_call_addr: usize, // Address of system call instruction
    pub si_syscall: u32,     // Number of attempted system call
    pub si_arch: u32,        // Architecture of attempted system call
}
