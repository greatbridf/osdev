mod percpu;

pub use percpu::{alloc_percpu_area, set_percpu_area};

pub unsafe fn bootstrap_smp() {
    #[cfg(feature = "smp")]
    {
        super::arch::init::bootstrap_cpus();
    }
}
