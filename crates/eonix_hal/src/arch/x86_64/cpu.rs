use core::arch::asm;
use core::marker::PhantomPinned;
use core::mem::size_of;
use core::pin::Pin;
use core::sync::atomic::AtomicUsize;

use eonix_preempt::PreemptGuard;
use eonix_sync_base::LazyLock;

use super::gdt::{GDTEntry, GDT};
use super::interrupt::InterruptControl;
use super::trap::TrapContext;

pub static CPU_COUNT: AtomicUsize = AtomicUsize::new(0);

#[eonix_percpu::define_percpu]
static LOCAL_CPU: LazyLock<CPU> = LazyLock::new(CPU::new);

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[allow(non_camel_case_types)]
struct TSS_SP {
    low: u32,
    high: u32,
}

#[repr(C)]
pub(crate) struct TSS {
    _reserved1: u32,
    rsp: [TSS_SP; 3],
    _reserved2: u32,
    _reserved3: u32,
    ist: [TSS_SP; 7],
    _reserved4: u32,
    _reserved5: u32,
    _reserved6: u16,
    iomap_base: u16,
    _pinned: PhantomPinned,
}

/// Architecture-specific cpu status data.
pub struct CPU {
    cpuid: usize,
    gdt: GDT,
    tss: TSS,
    interrupt: InterruptControl,
}

impl CPU {
    pub fn new() -> Self {
        let (interrupt_control, cpuid) = InterruptControl::new();

        Self {
            cpuid,
            gdt: GDT::new(),
            tss: TSS::new(),
            interrupt: interrupt_control,
        }
    }

    /// Load GDT and TSS in place.
    ///
    /// # Safety
    /// Make sure preemption and interrupt are disabled before calling this function.
    pub(crate) unsafe fn init(mut self: Pin<&mut Self>) {
        let tss = &self.as_ref().get_ref().tss;
        let tss_addr = tss as *const _ as u64;

        let mut gdt = unsafe {
            // SAFETY: We don't move the field out.
            self.as_mut().map_unchecked_mut(|me| &mut me.gdt)
        };

        unsafe {
            // SAFETY: We don't move `gdt` out.
            gdt.as_mut().get_unchecked_mut().set_tss(tss_addr as u64);
        }
        gdt.load();

        let mut interrupt = unsafe {
            // SAFETY: We don't move the field out.
            self.as_mut().map_unchecked_mut(|me| &mut me.interrupt)
        };

        // SAFETY: `self` is pinned, so are its fields.
        interrupt.as_mut().setup_idt();
        interrupt.as_mut().setup_timer();
    }

    /// Bootstrap all CPUs.
    /// This should only be called on the BSP.
    pub unsafe fn bootstrap_cpus(&self) {
        self.interrupt.send_sipi();
    }

    pub unsafe fn load_interrupt_stack(self: Pin<&mut Self>, rsp: u64) {
        unsafe {
            self.map_unchecked_mut(|me| &mut me.tss)
                .set_rsp0(rsp + size_of::<TrapContext>() as u64);
        }
    }

    pub fn set_tls32(self: Pin<&mut Self>, desc: GDTEntry, base: u64) {
        unsafe {
            // SAFETY: We don't move the GDT object.
            self.get_unchecked_mut().gdt.set_tls32(desc);
        }

        const IA32_KERNEL_GS_BASE: u32 = 0xc0000102;
        wrmsr(IA32_KERNEL_GS_BASE, base);
    }

    pub fn cpuid(&self) -> usize {
        self.cpuid
    }

    pub fn end_of_interrupt(self: Pin<&mut Self>) {
        unsafe {
            // SAFETY: We don't move the `interrupt` field out.
            self.map_unchecked_mut(|me| &mut me.interrupt)
                .end_of_interrupt();
        }
    }

    pub fn local() -> PreemptGuard<Pin<&'static mut Self>> {
        unsafe {
            // SAFETY: We pass the reference into a `PreemptGuard`, which ensures
            //         that preemption is disabled.
            PreemptGuard::new(Pin::new_unchecked(LOCAL_CPU.as_mut().get_mut()))
        }
    }
}

impl TSS {
    pub fn new() -> Self {
        Self {
            _reserved1: 0,
            rsp: [TSS_SP { low: 0, high: 0 }; 3],
            _reserved2: 0,
            _reserved3: 0,
            ist: [TSS_SP { low: 0, high: 0 }; 7],
            _reserved4: 0,
            _reserved5: 0,
            _reserved6: 0,
            iomap_base: 0,
            _pinned: PhantomPinned,
        }
    }

    pub fn set_rsp0(self: Pin<&mut Self>, rsp: u64) {
        unsafe {
            // SAFETY: We don't move the TSS object.
            let me = self.get_unchecked_mut();
            me.rsp[0].low = rsp as u32;
            me.rsp[0].high = (rsp >> 32) as u32;
        }
    }
}

#[inline(always)]
pub fn halt() {
    unsafe {
        asm!("hlt", options(att_syntax, nostack));
    }
}

#[inline(always)]
pub fn rdmsr(msr: u32) -> u64 {
    let edx: u32;
    let eax: u32;

    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") eax,
            out("edx") edx,
            options(att_syntax),
        );
    }

    (edx as u64) << 32 | eax as u64
}

#[inline(always)]
pub fn wrmsr(msr: u32, value: u64) {
    let eax = value as u32;
    let edx = (value >> 32) as u32;

    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") eax,
            in("edx") edx,
            options(att_syntax),
        );
    }
}
