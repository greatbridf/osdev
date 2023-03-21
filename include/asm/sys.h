#pragma once

#include <kernel/mem.h>
#include <types/types.h>

#ifdef __cplusplus
extern "C" {
#endif

void asm_switch_pd(pd_t pd_addr);
void asm_enable_paging(pd_t pd_addr);

pptr_t current_pd(void);

// the limit should be set on the higher 16bit
// e.g. (n * sizeof(segment_descriptor) - 1) << 16
void asm_load_gdt(uint32_t limit, pptr_t addr);

void asm_load_tr(uint16_t index);

extern const uint32_t kernel_size;
extern char* const bss_addr;
extern const uint32_t bss_len;

#ifdef __cplusplus
}
#endif
