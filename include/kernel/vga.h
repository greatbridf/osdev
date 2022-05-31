#pragma once
#ifndef _KERNEL_VGA_H_
#define _KERNEL_VGA_H_

#include <types/stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define VGA_CHAR_COLOR_WHITE (0x0fU)

struct vga_char {
    int8_t c;
    uint8_t color;
};

#define VGA_MEM ((struct vga_char*)0xb8000)
#define VGA_SCREEN_WIDTH_IN_CHARS (80U)
#define VGA_SCREEN_HEIGHT_IN_CHARS (25U)

void vga_put_char(struct vga_char* c);
void vga_print(const char* str, uint8_t color);

#ifdef __cplusplus
}
#endif

#endif // _KERNEL_VGA_H_
