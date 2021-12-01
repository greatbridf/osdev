#pragma once

#include <kernel/mem.h>

#ifdef __cplusplus
extern "C" {
#endif

void asm_enable_paging(page_directory_entry* pd_addr);

#ifdef __cplusplus
}
#endif
