use core::arch::asm;

use crate::task::TSS;

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct GDTEntry(u64);

pub struct GDT([GDTEntry; GDT::LEN]);

impl GDTEntry {
    const NULL: Self = Self(0);

    const KERNEL_CODE64: Self = Self::new(0, 0, 0x9a, 0x2);
    const KERNEL_DATA64: Self = Self::new(0, 0, 0x92, 0x0);

    const USER_CODE64: Self = Self::new(0, 0, 0xfa, 0x2);
    const USER_DATA64: Self = Self::new(0, 0, 0xf2, 0x0);

    const USER_CODE32: Self = Self::new(0, 0xfffff, 0xfa, 0xc);
    const USER_DATA32: Self = Self::new(0, 0xfffff, 0xf2, 0xc);

    pub const fn new(base: u32, limit: u32, access: u8, flags: u8) -> Self {
        let mut entry = 0u64;
        entry |= (limit & 0x0000_ffff) as u64;
        entry |= ((limit & 0x000f_0000) as u64) << 32;
        entry |= ((base & 0x00ff_ffff) as u64) << 16;
        entry |= ((base & 0xff00_0000) as u64) << 32;
        entry |= (access as u64) << 40;
        entry |= (flags as u64) << 52;

        GDTEntry(entry)
    }

    pub const fn new_ldt(base: u64, limit: u32) -> [Self; 2] {
        let first = Self::new(base as u32, limit, 0x82, 0x0);
        let second = Self(base >> 32);
        [first, second]
    }

    pub const fn new_tss(base: u64, limit: u32) -> [Self; 2] {
        let first = Self::new(base as u32, limit, 0x89, 0x0);
        let second = Self(base >> 32);
        [first, second]
    }
}

impl GDT {
    const LEN: usize = 10;
    const TLS32_INDEX: usize = 7;
    const TSS_INDEX: usize = 8;

    pub fn new() -> Self {
        Self([
            GDTEntry::NULL,
            GDTEntry::KERNEL_CODE64,
            GDTEntry::KERNEL_DATA64,
            GDTEntry::USER_CODE64,
            GDTEntry::USER_DATA64,
            GDTEntry::USER_CODE32,
            GDTEntry::USER_DATA32,
            GDTEntry::NULL, // User TLS 32bit
            GDTEntry::NULL, // TSS Descriptor Low
            GDTEntry::NULL, // TSS Descriptor High
        ])
    }

    pub fn set_tss(&mut self, base: u64) {
        let tss = GDTEntry::new_tss(base, size_of::<TSS>() as u32 - 1);
        self.0[Self::TSS_INDEX] = tss[0];
        self.0[Self::TSS_INDEX + 1] = tss[1];
    }

    pub fn set_tls32(&mut self, desc: GDTEntry) {
        self.0[7] = desc;
    }

    pub unsafe fn load(&self) {
        let len = Self::LEN * 8 - 1;
        let descriptor: [u64; 2] = [(len as u64) << 48, self.0.as_ptr() as u64];
        assert!(len < 0x10000, "GDT too large");

        let descriptor_address = &descriptor as *const _ as usize + 6;
        asm!(
            "lgdt ({})",
            "ltr %ax",
            in(reg) descriptor_address,
            in("ax") Self::TSS_INDEX as u16 * 8,
            options(att_syntax)
        );
    }
}
