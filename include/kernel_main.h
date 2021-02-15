#pragma once

#define MAKE_BREAK_POINT() asm volatile("xchgw %bx, %bx")

void kernel_main(void);
