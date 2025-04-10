use core::pin::Pin;

use super::{CPU, GDTEntry};

#[derive(Debug, Clone)]
pub enum UserTLS {
    /// TODO: This is not used yet.
    #[allow(dead_code)]
    TLS64(u64),
    TLS32 {
        base: u64,
        desc: GDTEntry,
    },
}

impl UserTLS {
    /// # Return
    /// Returns the TLS descriptor and the index of the TLS segment.
    pub fn new32(base: u32, limit: u32, is_limit_in_pages: bool) -> (Self, u32) {
        let flags = if is_limit_in_pages { 0xc } else { 0x4 };

        (
            Self::TLS32 {
                base: base as u64,
                desc: GDTEntry::new(base, limit, 0xf2, flags),
            },
            7,
        )
    }

    pub fn load(&self, cpu_status: Pin<&mut CPU>) {
        match self {
            Self::TLS64(base) => {
                const IA32_KERNEL_GS_BASE: u32 = 0xc0000102;
                super::wrmsr(IA32_KERNEL_GS_BASE, *base);
            }
            Self::TLS32 { base, desc } => {
                unsafe {
                    // SAFETY: We don't move the CPUStatus object.
                    let cpu_mut = cpu_status.get_unchecked_mut();
                    cpu_mut.set_tls32(*desc);
                }

                const IA32_KERNEL_GS_BASE: u32 = 0xc0000102;
                super::wrmsr(IA32_KERNEL_GS_BASE, *base);
            }
        }
    }
}

pub unsafe fn load_interrupt_stack(cpu_status: Pin<&mut CPU>, stack: u64) {
    // SAFETY: We don't move the CPUStatus object.
    cpu_status.get_unchecked_mut().set_rsp0(stack);
}
