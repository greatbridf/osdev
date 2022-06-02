#pragma once

#include <kernel/mem.h>
#include <types/types.h>

#ifdef __cplusplus
extern "C" {
#endif

void asm_enable_paging(page_directory_entry* pd_addr);

// the limit should be set on the higher 16bit
// e.g. (n * sizeof(segment_descriptor) - 1) << 16
void asm_load_gdt(uint32_t limit, phys_ptr_t addr);

void asm_load_tr(uint16_t index);

extern void* __real_kernel_end;

#ifdef __cplusplus
}
#endif
