#pragma once

static inline void __break_point(void)
{
    asm volatile("xchgw %bx, %bx");
}

#define MAKE_BREAK_POINT() __break_point()

#define KERNEL_STACK_SIZE (16 * 1024)
#define KERNEL_STACK_SEGMENT (0x10)

#define KERNEL_START_ADDR (0x00100000)

void kernel_main(void);
