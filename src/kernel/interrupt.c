#define _INTERRUPT_C_

#include <asm/port_io.h>
#include <kernel/interrupt.h>
#include <kernel/stdio.h>
#include <kernel/vga.h>

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
