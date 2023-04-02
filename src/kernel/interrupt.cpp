#define _INTERRUPT_C_

#include <asm/port_io.h>
#include <assert.h>
#include <kernel/hw/keyboard.h>
#include <kernel/hw/serial.h>
#include <kernel/hw/timer.h>
#include <kernel/interrupt.h>
#include <kernel/log.hpp>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/syscall.hpp>
#include <kernel/vfs.hpp>
#include <kernel/vga.hpp>
#include <stdint.h>
#include <stdio.h>
#include <types/size.h>
#include <types/types.h>

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
    init_syscall();

    uint16_t idt_descriptor[3];
    idt_descriptor[0] = sizeof(struct IDT_entry) * 256;
    *((uint32_t*)(idt_descriptor + 1)) = (ptr_t)IDT;

    asm_load_idt(idt_descriptor, 0);
}

SECTION(".text.kinit")
void init_pic(void)
{
    asm_cli();

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
    char buf[128] = {};
    snprintf(buf, sizeof(buf),
        "[kernel] int6 data: cs: %x, eflags: %x\n", cs, eflags);
    kmsg(buf);
    if (!current_process->attr.system)
        kill_current(-1);
    else
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
    char buf[128] = {};
    snprintf(buf, sizeof(buf),
        "[kernel] int13 data: error_code: %x, cs: %x, eflags: %x\n",
        error_code, cs, eflags);
    kmsg(buf);
    if (!current_process->attr.system)
        kill_current(-1);
    else
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
    char buf[256] {};
    snprintf(buf, 256, "Segmentation Fault (pid%d killed)\n", current_process->pid);
    kmsg(buf);
    kill_current(-1);
}

// page fault
extern "C" void int14_handler(int14_data* d)
{
    kernel::mm_list* mms = nullptr;
    if (current_process)
        mms = &current_process->mms;
    else
        mms = kernel_mms;

    auto mm_area = mms->find(d->l_addr);
    if (unlikely(mm_area == mms->end())) {
        if (d->error_code.user) {
            // user access of address that does not exist
            _int14_kill_user();
        } else {
            _int14_panic(d->v_eip, d->l_addr, d->error_code);
        }
    }
    if (unlikely(d->error_code.user && mm_area->attr.in.system))
        _int14_kill_user();

    page* page = &mm_area->pgs->at(vptrdiff(d->l_addr, mm_area->start) / PAGE_SIZE);
    kernel::paccess pa(page->pg_pteidx >> 12);
    auto pt = (pt_t)pa.ptr();
    assert(pt);
    pte_t* pte = *pt + (page->pg_pteidx & 0xfff);

    if (unlikely(d->error_code.present == 0 && !mm_area->mapped_file))
        _int14_panic(d->v_eip, d->l_addr, d->error_code);

    // copy on write
    if (page->attr.in.cow == 1) {
        // if it is a dying page
        if (*page->ref_count == 1) {
            page->attr.in.cow = 0;
            pte->in.a = 0;
            pte->in.rw = mm_area->attr.in.write;
            return;
        }
        // duplicate the page
        page_t new_page = __alloc_raw_page();

        // memory mapped
        if (d->error_code.present == 0)
            pte->in.p = 1;

        kernel::paccess pdst(new_page), psrc(page->phys_page_id);
        auto* new_page_data = (char*)pdst.ptr();
        auto* src = psrc.ptr();
        assert(new_page_data && src);
        memcpy(new_page_data, src, PAGE_SIZE);

        pte->in.page = new_page;
        pte->in.rw = mm_area->attr.in.write;
        pte->in.a = 0;

        --*page->ref_count;

        page->ref_count = types::pnew<types::kernel_ident_allocator>(page->ref_count, 1);
        page->attr.in.cow = 0;
        page->phys_page_id = new_page;

        // memory mapped
        if (d->error_code.present == 0) {
            size_t offset = align_down<12>((uint32_t)d->l_addr);
            offset -= (uint32_t)mm_area->start;

            int n = vfs_read(
                mm_area->mapped_file,
                new_page_data,
                PAGE_SIZE,
                mm_area->file_offset + offset,
                PAGE_SIZE);

            // TODO: send SIGBUS if offset is greater than real size
            if (n != PAGE_SIZE)
                memset(new_page_data + n, 0x00, PAGE_SIZE - n);
        }
    }
}

void after_irq(void)
{
    check_signal();
}

extern "C" void irq0_handler(interrupt_stack*)
{
    inc_tick();
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
    schedule();
}
// keyboard interrupt
extern "C" void irq1_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    handle_keyboard_interrupt();
    after_irq();
}
extern "C" void irq2_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
extern "C" void irq3_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
extern "C" void irq4_handler(void)
{
    // TODO: register interrupt handler in serial port driver
    serial_receive_data_interrupt();
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
extern "C" void irq5_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
extern "C" void irq6_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
extern "C" void irq7_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
extern "C" void irq8_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
extern "C" void irq9_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
extern "C" void irq10_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
extern "C" void irq11_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
extern "C" void irq12_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
extern "C" void irq13_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
extern "C" void irq14_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
extern "C" void irq15_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    after_irq();
}
