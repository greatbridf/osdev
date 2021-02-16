#define _INTERRUPT_C_

#include <asm/port_io.h>
#include <kernel/interrupt.h>
#include <kernel/stdio.h>
#include <kernel/vga.h>
#include <kernel_main.h>

static struct IDT_entry IDT[256];

void init_idt()
{
    asm_outb(0x20, 0x11); // edge trigger mode
    asm_outb(0x21, 0x20); // start from int 0x20
    asm_outb(0x21, 0x04); // PIC1 is connected to IRQ2 (1 << 2)
    asm_outb(0x21, 0x01); // no buffer mode

    asm_outb(0xa0, 0x11); // edge trigger mode
    asm_outb(0xa1, 0x28); // start from int 0x28
    asm_outb(0xa1, 0x02); // connected to IRQ2
    asm_outb(0xa1, 0x01); // no buffer mode

    // allow all the interrupts
    asm_outb(0x21, 0x00);
    asm_outb(0xa1, 0x00);

    // handle general protection fault (handle segmentation fault)
    SET_IDT_ENTRY_FN(13, int13, 0x08);
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

void int13_handler(
    struct regs_32 s_regs,
    uint32_t error_code,
    ptr_t eip,
    uint16_t cs,
    uint32_t eflags)
{
    char buf[512] = { 0 };

    vga_printk("---- SEGMENTATION FAULT ----\n", 0x0fu);

    snprintf(
        buf, 512,
        "eax: %d, ebx: %d, ecx: %d, edx: %d\n"
        "esp: %d, ebp: %d, esi: %d, edi: %d\n"
        "eip: %d, cs: %d, error_code: %d   \n"
        "eflags: %d                        \n",
        s_regs.eax, s_regs.ebx, s_regs.ecx,
        s_regs.edx, s_regs.esp, s_regs.ebp,
        s_regs.esi, s_regs.edi, eip,
        cs, error_code, eflags);
    vga_printk(buf, 0x0fu);

    vga_printk("----   HALTING SYSTEM   ----", 0x0fu);

    asm_cli();
    asm_hlt();
}

void irq0_handler(void)
{
    asm_outb(0x20, 0x20);
}
// keyboard interrupt
void irq1_handler(void)
{
    asm_outb(0x20, 0x20);
    uint8_t c = 0x00;
    c = asm_inb(PORT_KEYDATA);
    static char buf[4] = { 0 };
    snprintf(buf, 4, "%d", c);
    vga_printk(buf, 0x0fu);
}
void irq2_handler(void)
{
    asm_outb(0x20, 0x20);
}
void irq3_handler(void)
{
    asm_outb(0x20, 0x20);
}
void irq4_handler(void)
{
    asm_outb(0x20, 0x20);
}
void irq5_handler(void)
{
    asm_outb(0x20, 0x20);
}
void irq6_handler(void)
{
    asm_outb(0x20, 0x20);
}
void irq7_handler(void)
{
    asm_outb(0x20, 0x20);
}
void irq8_handler(void)
{
    asm_outb(0xa0, 0x20);
    asm_outb(0x20, 0x20);
}
void irq9_handler(void)
{
    asm_outb(0xa0, 0x20);
    asm_outb(0x20, 0x20);
}
void irq10_handler(void)
{
    asm_outb(0xa0, 0x20);
    asm_outb(0x20, 0x20);
}
void irq11_handler(void)
{
    asm_outb(0xa0, 0x20);
    asm_outb(0x20, 0x20);
}
void irq12_handler(void)
{
    asm_outb(0xa0, 0x20);
    asm_outb(0x20, 0x20);
}
void irq13_handler(void)
{
    asm_outb(0xa0, 0x20);
    asm_outb(0x20, 0x20);
}
void irq14_handler(void)
{
    asm_outb(0xa0, 0x20);
    asm_outb(0x20, 0x20);
}
void irq15_handler(void)
{
    asm_outb(0xa0, 0x20);
    asm_outb(0x20, 0x20);
}
