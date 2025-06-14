use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(target_arch = "riscv64")] {
        pub use crate::arch::fdt::FDT;
    }
}
