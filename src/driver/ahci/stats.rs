use core::sync::atomic::{AtomicUsize, Ordering};

pub struct AdapterPortStats {
    /// Number of commands sent
    cmd_sent: AtomicUsize,

    /// Number of transmission errors
    cmd_error: AtomicUsize,

    /// Number of interrupts fired
    int_fired: AtomicUsize,
}

impl AdapterPortStats {
    pub const fn new() -> Self {
        Self {
            cmd_sent: AtomicUsize::new(0),
            cmd_error: AtomicUsize::new(0),
            int_fired: AtomicUsize::new(0),
        }
    }

    pub fn inc_int_fired(&self) {
        self.int_fired.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_cmd_sent(&self) {
        self.cmd_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_cmd_error(&self) {
        self.cmd_error.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_int_fired(&self) -> usize {
        self.int_fired.load(Ordering::Relaxed)
    }

    pub fn get_cmd_sent(&self) -> usize {
        self.cmd_sent.load(Ordering::Relaxed)
    }

    pub fn get_cmd_error(&self) -> usize {
        self.cmd_error.load(Ordering::Relaxed)
    }
}
