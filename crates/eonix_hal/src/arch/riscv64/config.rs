/// mm
pub mod mm {
    pub const ROOT_PAGE_TABLE_PHYS_ADDR: usize = 0x8020_1000;
    pub const PHYS_MAP_VIRT: usize = 0xffff_ff00_0000_0000;
    pub const KIMAGE_PHYS_BASE: usize = 0x8020_0000;
    pub const KIMAGE_OFFSET: usize = 0xffff_ffff_0000_0000;
    pub const MMIO_VIRT_BASE: usize = KIMAGE_OFFSET;
    pub const KIMAGE_VIRT_BASE: usize = KIMAGE_OFFSET + KIMAGE_PHYS_BASE;
    pub const PAGE_SIZE: usize = 1 << PAGE_SIZE_BITS;
    pub const PAGE_SIZE_BITS: usize = 12;
    // 128GB
    pub const MEMORY_SIZE: usize = 0x20_0000_0000;

    pub const PTE_SIZE: usize = 8;
    pub const PTES_PER_PAGE: usize = PAGE_SIZE / PTE_SIZE;
    pub const ROOT_PAGE_TABLE_PFN: usize = ROOT_PAGE_TABLE_PHYS_ADDR >> 12;
    pub const PAGE_TABLE_PHYS_END: usize = 0x8080_0000;
}

pub mod platform {
    pub mod virt {
        pub const PLIC_BASE: usize = 0x0C00_0000;

        pub const PLIC_ENABLE_PER_HART_OFFSET: usize = 0x80; // 每个 Hart使能块 0x80 字节 (128 字节)

        pub const PLIC_THRESHOLD_CLAIM_COMPLETE_PER_HART_OFFSET: usize = 0x1000; // 每个 Hart/上下文块 0x1000 字节 (4KB)

        pub const PLIC_PRIORITY_OFFSET: usize = 0x0000_0000;
        pub const PLIC_PENDING_OFFSET: usize = 0x0000_1000;
        pub const PLIC_ENABLE_OFFSET: usize = 0x0000_2000; // Varies by context and mode (M/S/U)
        pub const PLIC_THRESHOLD_OFFSET: usize = 0x0020_0000; // Varies by context and mode (M/S/U)
        pub const PLIC_CLAIM_COMPLETE_OFFSET: usize = 0x0020_0004; // Varies by context and mode (M/S/U)

        // PLIC Context IDs for S-mode (assuming hart 0's S-mode context is 1, hart 1's is 3, etc.)
        // A common pattern is: context_id = hart_id * 2 + 1 (for S-mode)
        pub const PLIC_S_MODE_CONTEXT_STRIDE: usize = 2;

        // CLINT (Core Local Interruptor) memory-mapped registers
        // Base address for CLINT on QEMU virt platform
        pub const CLINT_BASE: usize = 0x200_0000;
        pub const CLINT_MSIP_OFFSET: usize = 0x0000; // Machine-mode Software Interrupt Pending (MSIP)
        pub const CLINT_MTIMECMP_OFFSET: usize = 0x4000; // Machine-mode Timer Compare (MTIMECMP)
        pub const CLINT_MTIME_OFFSET: usize = 0xBFF8;
        // TODO: this should get in fdt
        pub const CPU_FREQ_HZ: u64 = 10_000_000;
    }
}

pub mod time {
    pub const INTERRUPTS_PER_SECOND: usize = 1000;
}
