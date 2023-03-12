#define _INTERRUPT_C_

#include <asm/port_io.h>
#include <kernel/hw/keyboard.h>
#include <kernel/hw/serial.h>
#include <kernel/hw/timer.h>
#include <kernel/interrupt.h>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/stdio.hpp>
#include <kernel/syscall.hpp>
#include <kernel/vfs.hpp>
#include <kernel/vga.hpp>
#include <kernel_main.hpp>
#include <types/assert.h>
#include <types/size.h>
#include <types/stdint.h>
#include <types/types.h>

static struct IDT_entry IDT[256];

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
    struct regs_32 s_regs,
    ptr_t eip,
    uint16_t cs,
    uint32_t eflags)
{
    char buf[512];

    kmsg("\n---- INVALID OPCODE ----\n");

    snprintf(
        buf, 512,
        "eax: %x, ebx: %x, ecx: %x, edx: %x\n"
        "esp: %x, ebp: %x, esi: %x, edi: %x\n"
        "eip: %x, cs: %x, eflags: %x       \n",
        s_regs.eax, s_regs.ebx, s_regs.ecx,
        s_regs.edx, s_regs.esp, s_regs.ebp,
        s_regs.esi, s_regs.edi, eip,
        cs, eflags);
    kmsg(buf);

    kmsg("----   HALTING SYSTEM   ----\n");

    asm_cli();
    asm_hlt();
}

// general protection
extern "C" void int13_handler(
    struct regs_32 s_regs,
    uint32_t error_code,
    ptr_t eip,
    uint16_t cs,
    uint32_t eflags)
{
    char buf[512];

    kmsg("\n---- SEGMENTATION FAULT ----\n");

    snprintf(
        buf, 512,
        "eax: %x, ebx: %x, ecx: %x, edx: %x\n"
        "esp: %x, ebp: %x, esi: %x, edi: %x\n"
        "eip: %x, cs: %x, error_code: %x   \n"
        "eflags: %x                        \n",
        s_regs.eax, s_regs.ebx, s_regs.ecx,
        s_regs.edx, s_regs.esp, s_regs.ebp,
        s_regs.esi, s_regs.edi, eip,
        cs, error_code, eflags);
    kmsg(buf);

    kmsg("----   HALTING SYSTEM   ----\n");

    asm_cli();
    asm_hlt();
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
    char buf[256] {};
    snprintf(
        buf, 256,
        "\nkilled: segmentation fault (eip: %x, cr2: %x, error_code: %x)\n", eip, cr2, error_code);
    kmsg(buf);
    assert(false);
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
    if (unlikely(mm_area == mms->end()))
        _int14_panic(d->v_eip, d->l_addr, d->error_code);

    pte_t* pte = to_pte(mms->m_pd, d->l_addr);
    page* page = lto_page(&mm_area, d->l_addr);

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
        page_t new_page = alloc_raw_page();

        // memory mapped
        if (d->error_code.present == 0)
            pte->in.p = 1;

        char* new_page_data = (char*)to_vp(new_page);
        memcpy(new_page_data, to_vp(page->phys_page_id), PAGE_SIZE);

        pte->in.page = new_page;
        pte->in.rw = mm_area->attr.in.write;
        pte->in.a = 0;

        --*page->ref_count;

        page->ref_count = (size_t*)ki_malloc(sizeof(size_t));
        *page->ref_count = 1;
        page->attr.in.cow = 0;
        page->phys_page_id = new_page;

        // memory mapped
        if (d->error_code.present == 0) {
            size_t offset = vptrdiff(d->l_addr, mm_area->start) & 0xfffff000;
            vfs_read(mm_area->mapped_file, new_page_data, PAGE_SIZE, mm_area->file_offset + offset, PAGE_SIZE);
        }
    }
}

extern "C" void irq0_handler(interrupt_stack*)
{
    inc_tick();
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    schedule();
}
// keyboard interrupt
extern "C" void irq1_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    handle_keyboard_interrupt();
}
extern "C" void irq2_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
extern "C" void irq3_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
extern "C" void irq4_handler(void)
{
    // TODO: register interrupt handler in serial port driver
    serial_receive_data_interrupt();
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
extern "C" void irq5_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
extern "C" void irq6_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
extern "C" void irq7_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
extern "C" void irq8_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
extern "C" void irq9_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
extern "C" void irq10_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
extern "C" void irq11_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
extern "C" void irq12_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
extern "C" void irq13_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
extern "C" void irq14_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
extern "C" void irq15_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
