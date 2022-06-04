#pragma once

#include <kernel/mem.h>

#ifdef __cplusplus
extern "C" {
#endif

struct process {
    struct mm* mm;
};

#ifdef __cplusplus
}
#endif
