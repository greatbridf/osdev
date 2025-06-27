pub mod ahci;
pub mod e1000e;
pub mod serial;

#[cfg(target_arch = "riscv64")]
pub mod virtio;

#[cfg(target_arch = "riscv64")]
pub mod sbi_console;

#[cfg(target_arch = "riscv64")]
pub mod goldfish_rtc;
