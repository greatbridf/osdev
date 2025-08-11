mod hal;
mod virtio_blk;
mod virtio_net;

#[cfg(not(any(target_arch = "riscv64", target_arch = "loongarch64")))]
compile_error!("VirtIO drivers are only supported on RISC-V and LoongArch64 architecture");

#[cfg(target_arch = "riscv64")]
mod riscv64;

#[cfg(target_arch = "loongarch64")]
mod loongarch64;

pub fn init_virtio_devices() {
    #[cfg(target_arch = "riscv64")]
    riscv64::init();

    #[cfg(target_arch = "loongarch64")]
    loongarch64::init();
}
