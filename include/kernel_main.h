#pragma once
#include <types/types.h>

#define __crash() asm volatile("ud2")

#ifdef __BOCHS_SYSTEM__
#define __break_point() asm volatile("xchgw %bx, %bx")
#else
#define __break_point() __crash()
#endif

#define MAKE_BREAK_POINT() __break_point()

#define KERNEL_STACK_SIZE (16 * 1024)
#define KERNEL_STACK_SEGMENT (0x10)

#define KERNEL_START_ADDR (0x00100000)

void NORETURN kernel_main(void);

#ifdef __cplusplus
// in kernel_main.c
extern "C" struct tss32_t tss;
#else
// in kernel_main.c
extern struct tss32_t tss;
#endif
