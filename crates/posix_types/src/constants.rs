/// Wait for any child process
pub const P_ALL: u32 = 0;

/// Wait for a specific process by PID
pub const P_PID: u32 = 1;

/// Wait for a specific process group by PGID
pub const P_PGID: u32 = 2;

/// Wait for a specific process by PID file descriptor
pub const P_PIDFD: u32 = 3;

/// Child exited normally
pub const CLD_EXITED: u32 = 1;

/// Child was killed by a signal
pub const CLD_KILLED: u32 = 2;

/// Child terminated and dumped core
pub const CLD_DUMPED: u32 = 3;

/// Child was traced by a signal
pub const CLD_TRAPPED: u32 = 4;

/// Child was stopped by a signal
pub const CLD_STOPPED: u32 = 5;

/// Child was continued by a signal
pub const CLD_CONTINUED: u32 = 6;
