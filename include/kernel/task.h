#pragma once

#include <types/types.h>

#ifdef __cplusplus
extern "C" {
#endif

struct tss32_t {
    uint32_t backlink, esp0, ss0, esp1, ss1, esp2, ss2, cr3;
    uint32_t eip, eflags, eax, ecx, edx, ebx, esp, ebp, esi, edi;
    uint32_t es, cs, ss, ds, fs, gs;
    uint32_t ldtr, iomap;
};

#ifdef __cplusplus
}
#endif
