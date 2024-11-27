pub mod init;
pub mod interrupt;

use arch::x86_64::{gdt::GDT, task::TSS};

// TODO!!!: This can be stored in the percpu area.
//          But we need to implement a guard that ensures that preemption is disabled
//          while we are accessing the percpu variables.
#[arch::define_percpu]
static GDT_OBJECT: Option<GDT> = None;

#[arch::define_percpu]
static TSS_OBJECT: Option<TSS> = None;

pub mod user {
    use crate::sync::preempt;
    use arch::x86_64::gdt::GDTEntry;

    pub struct InterruptStack(pub u64);

    #[derive(Debug, Clone)]
    pub enum TLS {
        /// TODO: This is not used yet.
        #[allow(dead_code)]
        TLS64(u64),
        TLS32 {
            base: u64,
            desc: GDTEntry,
        },
    }

    impl TLS {
        /// # Return
        /// Returns the TLS descriptor and the index of the TLS segment.
        pub fn new32(base: u32, limit: u32, is_limit_in_pages: bool) -> (Self, u32) {
            let flags = if is_limit_in_pages { 0xc } else { 0x4 };

            (
                TLS::TLS32 {
                    base: base as u64,
                    desc: GDTEntry::new(base, limit, 0xf2, flags),
                },
                7,
            )
        }

        pub fn load(&self) {
            match self {
                TLS::TLS64(base) => {
                    const IA32_KERNEL_GS_BASE: u32 = 0xc0000102;
                    arch::x86_64::task::wrmsr(IA32_KERNEL_GS_BASE, *base);
                }
                TLS::TLS32 { base, desc } => {
                    preempt::disable();
                    let gdt = unsafe {
                        super::GDT_OBJECT
                            .as_mut()
                            .as_mut()
                            .expect("GDT should be valid")
                    };
                    gdt.set_tls32(*desc);
                    preempt::enable();

                    const IA32_KERNEL_GS_BASE: u32 = 0xc0000102;
                    arch::x86_64::task::wrmsr(IA32_KERNEL_GS_BASE, *base);
                }
            }
        }
    }

    pub fn load_interrupt_stack(stack: InterruptStack) {
        preempt::disable();
        let tss = unsafe {
            super::TSS_OBJECT
                .as_mut()
                .as_mut()
                .expect("TSS should be valid")
        };
        tss.set_rsp0(stack.0);
        preempt::enable();
    }
}
