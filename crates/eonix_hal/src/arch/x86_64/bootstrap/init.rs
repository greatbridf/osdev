use crate::{
    arch::{
        bootstrap::{EARLY_GDT_DESCRIPTOR, KERNEL_PML4},
        cpu::CPU,
        mm::{ArchPhysAccess, GLOBAL_PAGE_TABLE, V_KERNEL_BSS_START},
    },
    bootstrap::BootStrapData,
    mm::{ArchMemory, ArchPagingMode, BasicPageAlloc, BasicPageAllocRef, ScopedAllocator},
};
use acpi::{platform::ProcessorState, AcpiHandler, AcpiTables, PhysicalMapping, PlatformInfo};
use arch::wrmsr;
use core::{
    alloc::Allocator,
    arch::{asm, global_asm},
    cell::RefCell,
    hint::spin_loop,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering},
};
use eonix_hal_traits::mm::Memory;
use eonix_mm::{
    address::{Addr as _, PAddr, PRange, PhysAccess, VRange},
    page_table::{PageAttribute, PagingMode, PTE as _},
    paging::{Page, PageAccess, PageAlloc, PAGE_SIZE},
};
use eonix_percpu::PercpuArea;

static BSP_PAGE_ALLOC: AtomicPtr<RefCell<BasicPageAlloc>> = AtomicPtr::new(core::ptr::null_mut());

static AP_COUNT: AtomicUsize = AtomicUsize::new(0);
static AP_STACK: AtomicUsize = AtomicUsize::new(0);
static AP_SEM: AtomicBool = AtomicBool::new(false);

global_asm!(
    r#"
    .pushsection .stage1.smp, "ax", @progbits
    .code16
    ljmp $0x0, $2f

    2:
    lgdt {early_gdt_descriptor}
    mov $0xc0000080, %ecx
    rdmsr
    or $0x901, %eax # set LME, NXE, SCE
    wrmsr

    mov %cr4, %eax
    or $0xa0, %eax # set PAE, PGE
    mov %eax, %cr4

    mov ${kernel_pml4}, %eax
    mov %eax, %cr3

    mov %cr0, %eax
    or $0x80010001, %eax # set PE, WP, PG
    mov %eax, %cr0

    ljmp $0x08, $2f

    .code64
    2:
    mov $0x10, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %ss

    xor %rax, %rax
    inc %rax
    mov ${ap_semaphore}, %rcx

    2:
    xchg %rax, (%rcx) # AcqRel
    cmp $0, %rax
    je 2f
    pause
    jmp 2b

    2:
    mov ${ap_stack}, %rcx

    2:
    mov (%rcx), %rsp # Acquire
    cmp $0, %rsp
    jne 2f
    pause
    jmp 2b

    2:
    xor %rbp, %rbp
    mov %rbp, (%rcx) # Relaxed

    mov ${ap_semaphore}, %rcx
    xchg %rax, (%rcx) # Release

    mov %rsp, %rdi
    push %rbp # NULL return address
    mov ${ap_entry}, %rax
    jmp *%rax

    .popsection
    "#,
    early_gdt_descriptor = sym EARLY_GDT_DESCRIPTOR,
    kernel_pml4 = const KERNEL_PML4,
    ap_semaphore = sym AP_SEM,
    ap_stack = sym AP_STACK,
    ap_entry = sym ap_entry,
    options(att_syntax),
);

fn enable_sse() {
    unsafe {
        asm!(
            "mov %cr0, %rax",
            "and $(~0xc), %rax",
            "or $0x22, %rax",
            "mov %rax, %cr0",
            "mov %cr4, %rax",
            "or $0x600, %rax",
            "mov %rax, %cr4",
            "fninit",
            out("rax") _,
            options(att_syntax, nomem, nostack)
        )
    }
}

fn setup_cpu(alloc: impl PageAlloc) {
    let mut percpu_area = PercpuArea::new(|layout| {
        // TODO: Use page size defined in `arch`.
        let page_count = layout.size().div_ceil(PAGE_SIZE);
        let page = Page::alloc_at_least_in(page_count, alloc);

        let ptr = ArchPhysAccess::get_ptr_for_page(&page).cast();
        page.into_raw();

        ptr
    });

    percpu_area.setup(|pointer| {
        wrmsr(0xC0000101, pointer.addr().get() as u64);

        unsafe {
            // SAFETY: %gs:0 points to the start of the percpu area.
            asm!(
                "movq {}, %gs:0",
                in(reg) pointer.addr().get(),
                options(nostack, preserves_flags, att_syntax)
            );
        }
    });

    let mut cpu = CPU::local();
    unsafe {
        // SAFETY: Preemption is disabled and interrupt MUST be disabled since
        //         we are doing this in the kernel initialization phase.
        cpu.as_mut().init();
    }

    percpu_area.register(cpu.cpuid());
}

fn setup_pic() {
    // TODO: Remove this when we have completely switched to APIC.
    pub struct Port8 {
        no: u16,
    }

    impl Port8 {
        pub const fn new(no: u16) -> Self {
            Self { no }
        }

        pub fn write(&self, data: u8) {
            arch::outb(self.no, data)
        }
    }

    const PIC1_COMMAND: Port8 = Port8::new(0x20);
    const PIC1_DATA: Port8 = Port8::new(0x21);
    const PIC2_COMMAND: Port8 = Port8::new(0xA0);
    const PIC2_DATA: Port8 = Port8::new(0xA1);

    // Initialize PIC
    PIC1_COMMAND.write(0x11); // edge trigger mode
    PIC1_DATA.write(0x20); // IRQ 0-7 offset
    PIC1_DATA.write(0x04); // cascade with slave PIC
    PIC1_DATA.write(0x01); // no buffer mode

    PIC2_COMMAND.write(0x11); // edge trigger mode
    PIC2_DATA.write(0x28); // IRQ 8-15 offset
    PIC2_DATA.write(0x02); // cascade with master PIC
    PIC2_DATA.write(0x01); // no buffer mode

    // Allow all IRQs
    PIC1_DATA.write(0x0);
    PIC2_DATA.write(0x0);
}

fn bootstrap_smp(alloc: impl Allocator, page_alloc: &RefCell<BasicPageAlloc>) {
    #[derive(Clone)]
    struct Handler;

    impl AcpiHandler for Handler {
        unsafe fn map_physical_region<T>(
            &self,
            physical_address: usize,
            size: usize,
        ) -> PhysicalMapping<Self, T> {
            unsafe {
                PhysicalMapping::new(
                    physical_address,
                    ArchPhysAccess::as_ptr(PAddr::from(physical_address)),
                    size,
                    size,
                    self.clone(),
                )
            }
        }

        fn unmap_physical_region<T>(_: &PhysicalMapping<Self, T>) {}
    }

    let acpi_tables = unsafe {
        // SAFETY: Probing for RSDP in BIOS memory should be fine.
        AcpiTables::search_for_rsdp_bios(Handler).unwrap()
    };

    let platform_info = PlatformInfo::new_in(&acpi_tables, &alloc).unwrap();
    let processor_info = platform_info.processor_info.unwrap();

    let ap_count = processor_info
        .application_processors
        .iter()
        .filter(|ap| !matches!(ap.state, ProcessorState::Disabled))
        .count();

    unsafe {
        CPU::local().bootstrap_cpus();
    }

    for current_count in 0..ap_count {
        let stack_range = {
            let page_alloc = BasicPageAllocRef::new(&page_alloc);

            let ap_stack = Page::alloc_order_in(4, page_alloc);
            let stack_range = ap_stack.range();
            ap_stack.into_raw();

            stack_range
        };

        // SAFETY: All the APs can see the allocator work done before this point.
        let old = BSP_PAGE_ALLOC.swap((&raw const *page_alloc) as *mut _, Ordering::Release);
        assert!(
            old.is_null(),
            "BSP_PAGE_ALLOC should be null before we release it"
        );

        // SAFETY: The AP reading the stack will see the allocation work.
        while let Err(_) = AP_STACK.compare_exchange_weak(
            0,
            stack_range.end().addr(),
            Ordering::Release,
            Ordering::Relaxed,
        ) {
            // Spin until we can set the stack pointer for the AP.
            spin_loop();
        }

        spin_loop();

        // SAFETY: Make sure if we read the AP count, the allocator MUST have been released.
        while AP_COUNT.load(Ordering::Acquire) == current_count {
            // Wait for the AP to finish its initialization.
            spin_loop();
        }

        // SAFETY: We acquire the work done by the AP.
        let old = BSP_PAGE_ALLOC.swap(core::ptr::null_mut(), Ordering::Acquire);
        assert_eq!(
            old as *const _, &raw const *page_alloc,
            "We should read the previously saved allocator"
        );
    }
}

pub extern "C" fn kernel_init() -> ! {
    let global_page_table = &GLOBAL_PAGE_TABLE;
    let paging_levels = ArchPagingMode::LEVELS;

    enable_sse();

    let real_allocator = RefCell::new(BasicPageAlloc::new());
    let alloc = BasicPageAllocRef::new(&real_allocator);

    unsafe extern "C" {
        fn BSS_LENGTH();
    }

    for range in ArchMemory::free_ram() {
        real_allocator.borrow_mut().add_range(range);
    }

    // Map kernel BSS
    for pte in global_page_table.iter_kernel_in(
        VRange::from(V_KERNEL_BSS_START).grow(BSS_LENGTH as usize),
        paging_levels,
        &alloc,
    ) {
        let attr = PageAttribute::PRESENT
            | PageAttribute::WRITE
            | PageAttribute::READ
            | PageAttribute::HUGE
            | PageAttribute::GLOBAL;

        let page = Page::alloc_in(&alloc);
        pte.set(page.into_raw(), attr.into());
    }

    unsafe {
        // SAFETY: We've just mapped the area with sufficient length.
        core::ptr::write_bytes(V_KERNEL_BSS_START.addr() as *mut (), 0, BSS_LENGTH as usize);
    }

    setup_cpu(&alloc);
    setup_pic();

    ScopedAllocator::new(&mut [0; 1024])
        .with_alloc(|mem_alloc| bootstrap_smp(mem_alloc, &real_allocator));

    unsafe extern "Rust" {
        fn _eonix_hal_main(_: BootStrapData) -> !;
    }

    let bootstrap_data = BootStrapData {
        early_stack: PRange::new(PAddr::from(0x6000), PAddr::from(0x80000)),
        allocator: Some(real_allocator),
    };

    unsafe {
        _eonix_hal_main(bootstrap_data);
    }
}

pub extern "C" fn ap_entry(stack_bottom: PAddr) -> ! {
    let stack_range = PRange::new(stack_bottom - (1 << 3) * PAGE_SIZE, stack_bottom);

    {
        // SAFETY: Acquire all the work done by the BSP and other APs.
        let alloc = loop {
            let alloc = BSP_PAGE_ALLOC.swap(core::ptr::null_mut(), Ordering::AcqRel);

            if !alloc.is_null() {
                break alloc;
            }
        };

        let ref_alloc = unsafe { &*alloc };
        setup_cpu(BasicPageAllocRef::new(&ref_alloc));

        // SAFETY: Release our allocation work.
        BSP_PAGE_ALLOC.store(alloc, Ordering::Release);
    }

    // SAFETY: Make sure the allocator is set before we increment the AP count.
    AP_COUNT.fetch_add(1, Ordering::Release);

    unsafe extern "Rust" {
        fn _eonix_hal_ap_main(stack_range: PRange) -> !;
    }

    unsafe {
        _eonix_hal_ap_main(stack_range);
    }
}
