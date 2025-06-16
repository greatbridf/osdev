pub mod ahci;
pub mod e1000e;
#[cfg(target_arch = "x86_64")]
pub mod serial;

#[cfg(target_arch = "riscv64")]
pub mod virtio;

#[cfg(target_arch = "riscv64")]
pub mod sbi_console;
