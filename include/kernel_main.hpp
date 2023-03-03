#pragma once
#include <types/types.h>

#define KERNEL_STACK_SIZE (16 * 1024)
#define KERNEL_STACK_SEGMENT (0x10)

#define KERNEL_START_ADDR (0x00100000)

// in kernel_main.cpp
extern struct tss32_t tss;
