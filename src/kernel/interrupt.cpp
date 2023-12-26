#include <list>
#include <vector>

#include <asm/port_io.h>
#include <assert.h>
#include <kernel/hw/keyboard.h>
#include <kernel/hw/serial.h>
#include <kernel/hw/timer.h>
#include <kernel/interrupt.h>
#include <kernel/irq.hpp>
#include <kernel/log.hpp>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/vfs.hpp>
#include <kernel/vga.hpp>
#include <stdint.h>
#include <stdio.h>
#include <types/size.h>
#include <types/types.h>

struct IDT_entry {
    uint16_t offset_low;
    uint16_t selector;
    uint8_t zero;
    uint8_t type_attr;
    uint16_t offset_high;
};

// interrupt stubs
extern "C" void irq0(); extern "C" void irq1(); extern "C" void irq2();
extern "C" void irq3(); extern "C" void irq4(); extern "C" void irq5();
extern "C" void irq6(); extern "C" void irq7(); extern "C" void irq8();
extern "C" void irq9(); extern "C" void irq10(); extern "C" void irq11();
extern "C" void irq12(); extern "C" void irq13(); extern "C" void irq14();
extern "C" void irq15(); extern "C" void int6(); extern "C" void int8();
extern "C" void int13(); extern "C" void int14();
extern "C" void syscall_stub();

#define SET_UP_IRQ(N, SELECTOR)                   \
    ptr_t addr_irq##N = (ptr_t)irq##N;            \
    set_idt_entry(IDT, 0x20 + (N), (addr_irq##N), \
        (SELECTOR), KERNEL_INTERRUPT_GATE_TYPE);

#define SET_IDT_ENTRY_FN(N, FUNC_NAME, SELECTOR, TYPE) \
    ptr_t addr_##FUNC_NAME = (ptr_t)FUNC_NAME;         \
    set_idt_entry(IDT, (N), (addr_##FUNC_NAME), (SELECTOR), (TYPE));

SECTION(".text.kinit")
static void set_idt_entry(IDT_entry (&idt)[256], int n,
    uintptr_t offset, uint16_t selector, uint8_t type)
{
    idt[n].offset_low = offset & 0xffff;
    idt[n].selector = selector;
    idt[n].zero = 0;
    idt[n].type_attr = type;
    idt[n].offset_high = (offset >> 16) & 0xffff;
}

// idt_descriptor: uint16_t[3]
// [0] bit 0 :15 => limit
// [1] bit 16:47 => address
extern "C" void asm_load_idt(uint16_t idt_descriptor[3], int sti);

static struct IDT_entry IDT[256];

static inline void NORETURN die(regs_32& regs, ptr_t eip)
{
    char buf[512] = {};
    snprintf(
        buf, sizeof(buf),
        "***** KERNEL PANIC *****\n"
        "eax: %x, ebx: %x, ecx: %x, edx: %x\n"
        "esp: %x, ebp: %x, esi: %x, edi: %x\n"
        "eip: %x\n",
        regs.eax, regs.ebx, regs.ecx,
        regs.edx, regs.esp, regs.ebp,
        regs.esi, regs.edi, eip);
    kmsg(buf);
    freeze();
}

SECTION(".text.kinit")
void init_idt()
{
    asm_cli();

    memset(IDT, 0x00, sizeof(IDT));

    // invalid opcode
    SET_IDT_ENTRY_FN(6, int6, 0x08, KERNEL_INTERRUPT_GATE_TYPE);
    // double fault
    SET_IDT_ENTRY_FN(8, int8, 0x08, KERNEL_INTERRUPT_GATE_TYPE);
    // general protection
    SET_IDT_ENTRY_FN(13, int13, 0x08, KERNEL_INTERRUPT_GATE_TYPE);
    // page fault
    SET_IDT_ENTRY_FN(14, int14, 0x08, KERNEL_INTERRUPT_GATE_TYPE);
    // system call
    SET_IDT_ENTRY_FN(0x80, syscall_stub, 0x08, USER_INTERRUPT_GATE_TYPE);

    uint16_t idt_descriptor[3];
    idt_descriptor[0] = sizeof(struct IDT_entry) * 256;
    *((uint32_t*)(idt_descriptor + 1)) = (ptr_t)IDT;

    asm_load_idt(idt_descriptor, 0);
}

using kernel::irq::irq_handler_t;
static std::vector<std::list<irq_handler_t>> s_irq_handlers;

void kernel::irq::register_handler(int irqno, irq_handler_t handler)
{
    s_irq_handlers[irqno].emplace_back(std::move(handler));
}

SECTION(".text.kinit")
void init_pic(void)
{
    asm_cli();

    s_irq_handlers.resize(16);

    // TODO: move this to timer driver
    kernel::irq::register_handler(0, []() {
        inc_tick();
        schedule();
    });

    asm_outb(PORT_PIC1_COMMAND, 0x11); // edge trigger mode
    asm_outb(PORT_PIC1_DATA, 0x20); // start from int 0x20
    asm_outb(PORT_PIC1_DATA, 0x04); // PIC1 is connected to IRQ2 (1 << 2)
    asm_outb(PORT_PIC1_DATA, 0x01); // no buffer mode

    asm_outb(PORT_PIC2_COMMAND, 0x11); // edge trigger mode
    asm_outb(PORT_PIC2_DATA, 0x28); // start from int 0x28
    asm_outb(PORT_PIC2_DATA, 0x02); // connected to IRQ2
    asm_outb(PORT_PIC2_DATA, 0x01); // no buffer mode

    // allow all the interrupts
    asm_outb(PORT_PIC1_DATA, 0x00);
    asm_outb(PORT_PIC2_DATA, 0x00);

    // 0x08 stands for kernel code segment
    SET_UP_IRQ(0, 0x08);
    SET_UP_IRQ(1, 0x08);
    SET_UP_IRQ(2, 0x08);
    SET_UP_IRQ(3, 0x08);
    SET_UP_IRQ(4, 0x08);
    SET_UP_IRQ(5, 0x08);
    SET_UP_IRQ(6, 0x08);
    SET_UP_IRQ(7, 0x08);
    SET_UP_IRQ(8, 0x08);
    SET_UP_IRQ(9, 0x08);
    SET_UP_IRQ(10, 0x08);
    SET_UP_IRQ(11, 0x08);
    SET_UP_IRQ(12, 0x08);
    SET_UP_IRQ(13, 0x08);
    SET_UP_IRQ(14, 0x08);
    SET_UP_IRQ(15, 0x08);
}

extern "C" void int6_handler(
    regs_32 s_regs,
    ptr_t eip,
    uint16_t cs,
    uint32_t eflags)
{
    if (!current_process->attr.system)
        kill_current(SIGSEGV);

    char buf[128];
    snprintf(buf, sizeof(buf),
        "[kernel] int6 data: cs: %x, eflags: %x\n", cs, eflags);
    kmsg(buf);

    die(s_regs, eip);
}

// general protection
extern "C" void int13_handler(
    struct regs_32 s_regs,
    uint32_t error_code,
    ptr_t eip,
    uint16_t cs,
    uint32_t eflags)
{
    if (!current_process->attr.system)
        kill_current(SIGILL);

    char buf[128] = {};
    snprintf(buf, sizeof(buf),
        "[kernel] int13 data: error_code: %x, cs: %x, eflags: %x\n",
        error_code, cs, eflags);
    kmsg(buf);

    die(s_regs, eip);
}

struct PACKED int14_data {
    void* l_addr;
    struct regs_32 s_regs;
    struct page_fault_error_code error_code;
    void* v_eip;
    uint32_t cs;
    uint32_t eflags;
};

static inline void _int14_panic(void* eip, void* cr2, struct page_fault_error_code error_code)
{
    char buf[128] = {};
    snprintf(buf, sizeof(buf),
        "[kernel] int14 data: eip: %p, cr2: %p, error_code: %x\n"
        "[kernel] freezing...\n",
        eip, cr2, error_code);
    kmsg(buf);
    freeze();
}

static inline void NORETURN _int14_kill_user(void)
{
    kill_current(SIGSEGV);
}

// page fault
extern "C" void int14_handler(int14_data* d)
{
    kernel::memory::mm_list* mms = nullptr;
    if (current_process) [[likely]]
        mms = &current_process->mms;
    else
        mms = kernel::memory::mm_list::s_kernel_mms;

    auto* mm_area = mms->find(d->l_addr);
    if (!mm_area) [[unlikely]] {
        if (d->error_code.user) {
            // user access of address that does not exist
            _int14_kill_user();
        } else {
            _int14_panic(d->v_eip, d->l_addr, d->error_code);
        }
    }
    if (d->error_code.user && mm_area->attr.system)
        _int14_kill_user();

    page* page = &(*mm_area->pgs)[vptrdiff(d->l_addr, mm_area->start) / PAGE_SIZE];
    kernel::paccess pa(page->pg_pteidx >> 12);
    auto pt = (pt_t)pa.ptr();
    assert(pt);
    pte_t* pte = *pt + (page->pg_pteidx & 0xfff);

    if (unlikely(d->error_code.present == 0 && !mm_area->mapped_file))
        _int14_panic(d->v_eip, d->l_addr, d->error_code);

    if (page->attr & PAGE_COW) {
        // if it is a dying page
        if (*page->ref_count == 1) {
            page->attr &= ~PAGE_COW;
            pte->in.p = 1;
            pte->in.a = 0;
            pte->in.rw = mm_area->attr.write;
            return;
        }
        // duplicate the page
        page_t new_page = __alloc_raw_page();

        {
            kernel::paccess pdst(new_page), psrc(page->phys_page_id);
            auto* new_page_data = (char*)pdst.ptr();
            auto* src = psrc.ptr();
            assert(new_page_data && src);
            memcpy(new_page_data, src, PAGE_SIZE);
        }

        pte->in.page = new_page;
        pte->in.rw = mm_area->attr.write;
        pte->in.a = 0;

        --*page->ref_count;

        page->ref_count = types::memory::kinew<size_t>(1);
        page->attr &= ~PAGE_COW;
        page->phys_page_id = new_page;
    }

    if (page->attr & PAGE_MMAP) {
        pte->in.p = 1;

        size_t offset = align_down<12>((uint32_t)d->l_addr);
        offset -= (uint32_t)mm_area->start;

        kernel::paccess pa(page->phys_page_id);
        auto* data = (char*)pa.ptr();
        assert(data);

        int n = vfs_read(
            mm_area->mapped_file,
            data,
            PAGE_SIZE,
            mm_area->file_offset + offset,
            PAGE_SIZE);

        // TODO: send SIGBUS if offset is greater than real size
        if (n != PAGE_SIZE)
            memset(data + n, 0x00, PAGE_SIZE - n);

        page->attr &= ~PAGE_MMAP;
    }
}

extern "C" void irq_handler(
    int irqno,
    interrupt_stack* context,
    mmx_registers* mmxregs)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    if (irqno >= 8)
        asm_outb(PORT_PIC1_COMMAND, PIC_EOI);

    for (const auto& handler : s_irq_handlers[irqno])
        handler();

    if (context->cs != USER_CODE_SEGMENT)
        return;

    if (current_thread->signals.pending_signal())
        current_thread->signals.handle(context, mmxregs);
}
