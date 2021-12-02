#pragma once

#include <kernel/mem.h>

#ifdef __cplusplus
extern "C" {
#endif

void asm_enable_paging(page_directory_entry* pd_addr);

// the limit should be set on the higher 16bit
void asm_load_gdt(uint32_t limit, uint32_t addr);

void asm_load_tr(uint16_t index);

#ifdef __cplusplus
}
#endif
