/// mm
pub mod mm {
    pub const ROOT_PAGE_TABLE_PHYS_ADDR: usize = 0x8040_0000;
    pub const ROOT_PAGE_TABLE_PFN: usize = ROOT_PAGE_TABLE_PHYS_ADDR >> 12;
    pub const PAGE_TABLE_PHYS_END: usize = 0x8080_0000;
    pub const PHYS_MAP_VIRT: usize = 0xFFFF_FF00_0000_0000;
    pub const KIMAGE_PHYS_BASE: usize = 0x8020_0000;
    pub const KIMAGE_VIRT_BASE: usize = 0xFFFF_FFFF_FFC0_0000;
    pub const PAGE_SIZE: usize = 0x1000;
    #[derive(Clone, Copy)]
    pub enum PageSize {
        _4KbPage = 4096,
        _2MbPage = 2 * 1024 * 1024,
        _1GbPage = 1 * 1024 * 1024 * 1024,
    }
}

/// smp
pub mod smp {
    use spin::Once;

    pub static NUM_HARTS: Once<usize> = Once::new();

    pub fn set_num_harts(num: usize) {
        NUM_HARTS.call_once(|| num);
    }

    pub fn get_num_harts() -> usize {
        *NUM_HARTS.get().expect("NUM_HARTS should be initialized by now")
    }
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
        pub const CLINT_MSIP_OFFSET: usize = 0x0000;      // Machine-mode Software Interrupt Pending (MSIP)
        pub const CLINT_MTIMECMP_OFFSET: usize = 0x4000;  // Machine-mode Timer Compare (MTIMECMP)
        pub const CLINT_MTIME_OFFSET: usize = 0xBFF8;
        pub const CPU_FREQ_HZ: u64 = 10_000_000;
    }
}

