#define _KERNEL_VGA_C_
#include <types/stdint.h>

#include <kernel/vga.h>

static struct vga_char* p_vga_head = VGA_MEM;

void vga_put_char(struct vga_char* c)
{
    *p_vga_head = *c;
    ++p_vga_head;
}

void vga_printk(const int8_t* str, uint8_t color)
{
    struct vga_char s_c;
    s_c.color = color;
    while ((s_c.c = *(str++)) != 0x00) {
        vga_put_char(&s_c);
    }
}
