#pragma once
#ifndef _KERNEL_VGA_H_
#define _KERNEL_VGA_H_

#include <types/stdint.h>

struct vga_char {
    int8_t c;
    uint8_t color;
};

#define VGA_MEM ((struct vga_char*)0xb8000)
#define VGA_SCREEN_WIDTH_IN_CHARS (80U)
#define VGA_SCREEN_HEIGHT_IN_CHARS (25U)

void vga_put_char(struct vga_char* c);
void vga_printk(const int8_t* str, uint8_t color);

#endif // _KERNEL_VGA_H_
