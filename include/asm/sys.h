#pragma once

#include <kernel/mem.hpp>
#include <types/types.h>

#ifdef __cplusplus
extern "C" {
#endif

void asm_enable_paging(page_directory_entry* pd_addr);

phys_ptr_t current_pd(void);

// the limit should be set on the higher 16bit
// e.g. (n * sizeof(segment_descriptor) - 1) << 16
// the lower bit off the limit is either 0 or 1
// indicating whether or not to enable interrupt
// after loading gdt
void asm_load_gdt(uint32_t limit, phys_ptr_t addr);

void asm_load_tr(uint16_t index);

extern void* __real_kernel_end;

#ifdef __cplusplus
}
#endif
