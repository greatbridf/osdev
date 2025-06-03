use super::{enable_sse, GDTEntry, InterruptControl, GDT};
use core::{pin::Pin, ptr::addr_of};

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
        }
    }

    pub fn set_rsp0(&mut self, rsp: u64) {
        self.rsp[0].low = rsp as u32;
        self.rsp[0].high = (rsp >> 32) as u32;
    }
}

/// Architecture-specific cpu status data.
pub struct CPU {
    cpuid: usize,
    gdt: GDT,
    tss: TSS,
    pub interrupt: InterruptControl,
}

impl CPU {
    pub fn new(base: usize) -> Self {
        let (interrupt_control, cpuid) = InterruptControl::new(base);

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
    pub unsafe fn init(self: Pin<&mut Self>) {
        enable_sse();

        // SAFETY: We don't move the object.
        let self_mut = self.get_unchecked_mut();

        let tss_addr = addr_of!(self_mut.tss);
        self_mut.gdt.set_tss(tss_addr as u64);
        self_mut.gdt.load();

        // SAFETY: `self` is pinned, so are its fields.
        Pin::new_unchecked(&mut self_mut.interrupt).setup_idt();
        self_mut.interrupt.setup_timer();
    }

    /// Bootstrap all CPUs.
    /// This should only be called on the BSP.
    pub unsafe fn bootstrap_cpus(&self) {
        self.interrupt.send_sipi();
    }

    pub unsafe fn set_rsp0(&mut self, rsp: u64) {
        self.tss.set_rsp0(rsp);
    }

    pub unsafe fn set_tls32(&mut self, desc: GDTEntry) {
        self.gdt.set_tls32(desc);
    }

    pub fn cpuid(&self) -> usize {
        self.cpuid
    }
}

#[macro_export]
macro_rules! define_smp_bootstrap {
    ($cpu_count:literal, $ap_entry:ident, $alloc_kstack:tt) => {
        static BOOT_SEMAPHORE: core::sync::atomic::AtomicU64 =
            core::sync::atomic::AtomicU64::new(0);
        static BOOT_STACK: core::sync::atomic::AtomicU64 =
            core::sync::atomic::AtomicU64::new(0);

        static CPU_COUNT: core::sync::atomic::AtomicU64 =
            core::sync::atomic::AtomicU64::new(1);

        core::arch::global_asm!(
            r#"
        .pushsection .stage1.smp, "ax", @progbits
        .code16
        .globl ap_bootstrap
        .type ap_bootstrap, @function
        ap_bootstrap:
            ljmp $0x0, $2f

        2:
            # we use the shared gdt for cpu bootstrapping
            lgdt EARLY_GDT_DESCRIPTOR

            # set msr
            mov $0xc0000080, %ecx
            rdmsr
            or $0x901, %eax # set LME, NXE, SCE
            wrmsr

            # set cr4
            mov %cr4, %eax
            or $0xa0, %eax # set PAE, PGE
            mov %eax, %cr4

            # load new page table
            mov ${KERNEL_PML4}, %eax
            mov %eax, %cr3

            mov %cr0, %eax
            // SET PE, WP, PG
            or $0x80010001, %eax
            mov %eax, %cr0

            ljmp $0x08, $2f

        .code64
        2:
            mov $0x10, %ax
            mov %ax, %ds
            mov %ax, %es
            mov %ax, %ss

            xor %rsp, %rsp
            xor %rax, %rax
            inc %rax
        2:
            xchg %rax, {BOOT_SEMAPHORE}
            cmp $0, %rax
            je 2f
            pause
            jmp 2b

        2:
            mov {BOOT_STACK}, %rsp # Acquire
            cmp $0, %rsp
            jne 2f
            pause
            jmp 2b

        2:
            xor %rax, %rax
            mov %rax, {BOOT_STACK} # Release
            xchg %rax, {BOOT_SEMAPHORE}

            lock incq {CPU_COUNT}

            xor %rbp, %rbp
            push %rbp # NULL return address
            mov ${AP_ENTRY}, %rax
            jmp *%rax
            .popsection
            "#,
            KERNEL_PML4 = const 0x1000,
            BOOT_SEMAPHORE = sym BOOT_SEMAPHORE,
            BOOT_STACK = sym BOOT_STACK,
            CPU_COUNT = sym CPU_COUNT,
            AP_ENTRY = sym $ap_entry,
            options(att_syntax),
        );

        pub unsafe fn wait_cpus_online() {
            use core::sync::atomic::Ordering;
            while CPU_COUNT.load(Ordering::Acquire) != $cpu_count {
                if BOOT_STACK.load(Ordering::Acquire) == 0 {
                    let stack_bottom = $alloc_kstack as u64;
                    BOOT_STACK.store(stack_bottom, Ordering::Release);
                }
                $crate::pause();
            }
        }
    };
}
