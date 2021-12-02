#pragma once

#define MAKE_BREAK_POINT() asm volatile("xchgw %bx, %bx")

#define KERNEL_STACK_SIZE (16 * 1024)
#define KERNEL_STACK_SEGMENT (0x10)

void kernel_main(void);
