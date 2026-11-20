#[cfg(target_arch = "loongarch64")]
mod loongarch64;
#[cfg(target_arch = "riscv64")]
mod riscv64;
#[cfg(target_arch = "x86_64")]
mod x86_64;

#[allow(unused)]
#[cfg(target_arch = "loongarch64")]
pub use loongarch64::*;
#[allow(unused)]
#[cfg(target_arch = "riscv64")]
pub use riscv64::*;
#[allow(unused)]
#[cfg(target_arch = "x86_64")]
pub use x86_64::*;

#[allow(unused)]
fn check_arch() {
    // Arch should have PTE format defined
}
