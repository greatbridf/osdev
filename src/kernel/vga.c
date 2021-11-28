#define _KERNEL_VGA_C_
#include <types/stdint.h>

#include <kernel/stdio.h>
#include <kernel/vga.h>

static struct vga_char* p_vga_head = VGA_MEM;

void vga_put_char(struct vga_char* c)
{
    *p_vga_head = *c;
    ++p_vga_head;
    if ((p_vga_head - VGA_MEM) == 80 * 25) {
        p_vga_head = VGA_MEM;
    }
}

void vga_return()
{
    const int32_t offset = p_vga_head - VGA_MEM;
    p_vga_head -= (offset % VGA_SCREEN_WIDTH_IN_CHARS);
}

void vga_new_line()
{
    int32_t offset = p_vga_head - VGA_MEM;
    offset %= VGA_SCREEN_WIDTH_IN_CHARS;
    p_vga_head += (VGA_SCREEN_WIDTH_IN_CHARS - offset);
    if ((p_vga_head - VGA_MEM) >= 80 * 25) {
        p_vga_head = VGA_MEM;
    }
}

void vga_printk(const char* str, uint8_t color)
{
    struct vga_char s_c;
    s_c.color = color;
    while ((s_c.c = *(str++)) != 0x00) {
        switch (s_c.c) {
        case CR:
            vga_return();
            break;
        case LF:
            vga_new_line();
            break;
        default:
            vga_put_char(&s_c);
            break;
        }
    }
}
