#define _INTERRUPT_C_

#include <asm/port_io.h>
#include <kernel/hw/keyboard.h>
#include <kernel/hw/timer.h>
#include <kernel/interrupt.h>
#include <kernel/stdio.h>
#include <kernel/vga.h>
#include <kernel_main.h>

static struct IDT_entry IDT[256];

void init_idt()
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

    // handle general protection fault (handle segmentation fault)
    SET_IDT_ENTRY_FN(6, int6, 0x08);
    SET_IDT_ENTRY_FN(13, int13, 0x08);
    SET_IDT_ENTRY_FN(14, int14, 0x08);
    // SET_IDT_ENTRY(0x0c, /* addr */ 0, 0x08);

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

    uint16_t idt_descriptor[3];
    idt_descriptor[0] = sizeof(struct IDT_entry) * 256;
    *((uint32_t*)(idt_descriptor + 1)) = (ptr_t)IDT;

    asm_load_idt(idt_descriptor);
}

void int6_handler(
    struct regs_32 s_regs,
    uint32_t error_code,
    ptr_t eip,
    uint16_t cs)
{
    char buf[512];

    // TODO: change to tty
    vga_print("---- INVALID OPCODE ----\n", 0x0fu);

    snprintf(
        buf, 512,
        "eax: %x, ebx: %x, ecx: %x, edx: %x\n"
        "esp: %x, ebp: %x, esi: %x, edi: %x\n"
        "eip: %x, cs: %x, error_code: %x   \n",
        s_regs.eax, s_regs.ebx, s_regs.ecx,
        s_regs.edx, s_regs.esp, s_regs.ebp,
        s_regs.esi, s_regs.edi, eip,
        cs, error_code);
    // TODO: change to tty
    vga_print(buf, 0x0fu);

    // TODO: change to tty
    vga_print("----   HALTING SYSTEM   ----", 0x0fu);

    asm_cli();
    asm_hlt();
}

// general protection
void int13_handler(
    struct regs_32 s_regs,
    uint32_t error_code,
    ptr_t eip,
    uint16_t cs,
    uint32_t eflags)
{
    char buf[512];

    // TODO: change to tty
    vga_print("---- SEGMENTATION FAULT ----\n", 0x0fu);

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
    // TODO: change to tty
    vga_print(buf, 0x0fu);

    // TODO: change to tty
    vga_print("----   HALTING SYSTEM   ----", 0x0fu);

    asm_cli();
    asm_hlt();
}

// page fault
void int14_handler(
    ptr_t addr,
    struct regs_32 s_regs,
    uint32_t error_code,
    ptr_t eip,
    uint16_t cs,
    uint32_t eflags)
{
    char buf[512];

    // TODO: change to tty
    vga_print("---- PAGE FAULT ----\n", 0x0fu);

    snprintf(
        buf, 512,
        "eax: %x, ebx: %x, ecx: %x, edx: %x\n"
        "esp: %x, ebp: %x, esi: %x, edi: %x\n"
        "eip: %x, cs: %x, error_code: %x   \n"
        "eflags: %x, addr: %x              \n",
        s_regs.eax, s_regs.ebx, s_regs.ecx,
        s_regs.edx, s_regs.esp, s_regs.ebp,
        s_regs.esi, s_regs.edi, eip,
        cs, error_code, eflags, addr);
    // TODO: change to tty
    vga_print(buf, 0x0fu);

    // TODO: change to tty
    vga_print("----   HALTING SYSTEM   ----", 0x0fu);

    asm_cli();
    asm_hlt();
}

void irq0_handler(void)
{
    inc_tick();
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
// keyboard interrupt
void irq1_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
    handle_keyboard_interrupt();
}
void irq2_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
void irq3_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
void irq4_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
void irq5_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
void irq6_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
void irq7_handler(void)
{
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
void irq8_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
void irq9_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
void irq10_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
void irq11_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
void irq12_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
void irq13_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
void irq14_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
void irq15_handler(void)
{
    asm_outb(PORT_PIC2_COMMAND, PIC_EOI);
    asm_outb(PORT_PIC1_COMMAND, PIC_EOI);
}
