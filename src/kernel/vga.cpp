#define _KERNEL_VGA_C_

#include <stdint.h>
#include <string.h>

#include <kernel/vga.hpp>

static struct vga_char* p_vga_head = VGA_MEM;

static inline void vga_return() {
    const int32_t offset = p_vga_head - VGA_MEM;
    p_vga_head -= (offset % VGA_SCREEN_WIDTH_IN_CHARS);
}

static inline void vga_new_line() {
    int32_t offset = p_vga_head - VGA_MEM;
    offset %= VGA_SCREEN_WIDTH_IN_CHARS;
    p_vga_head += (VGA_SCREEN_WIDTH_IN_CHARS - offset);
    if ((p_vga_head - VGA_MEM) >= 80 * 25) {
        p_vga_head = VGA_MEM;
    }
}

static inline void real_vga_put_char(struct vga_char* c) {
    *p_vga_head = *c;
    ++p_vga_head;
    if ((p_vga_head - VGA_MEM) == 80 * 25) {
        p_vga_head = VGA_MEM;
    }
}

void vga_put_char(struct vga_char* c) {
    switch (c->c) {
        case CR:
            vga_return();
            break;
        case LF:
            vga_new_line();
            break;
        default:
            real_vga_put_char(c);
            break;
    }
}

void vga_print(const char* str, uint8_t color) {
    struct vga_char s_c;
    s_c.color = color;
    while ((s_c.c = *(str++)) != 0x00) {
        vga_put_char(&s_c);
    }
}
