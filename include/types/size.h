#pragma once

#include "stdint.h"

#define __32bit_system

#ifdef __32bit_system
typedef uint32_t ptr_t;
typedef int32_t diff_t;
#elif
typedef uint64_t ptr_t;
typedef int64_t diff_t;
#endif

typedef ptr_t phys_ptr_t;
typedef size_t page_t;
typedef size_t pd_i_t;
typedef size_t pt_i_t;
