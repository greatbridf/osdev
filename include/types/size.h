#pragma once

#include "stdint.h"

#ifdef __GNUC__
#define PACKED __attribute__((__packed__))
#else
#error "no definition for ((PACKED))"
#endif

#define __32bit_system

#ifdef __32bit_system
typedef uint32_t ptr_t;
typedef int32_t diff_t;
#elif
typedef uint64_t ptr_t;
typedef int64_t diff_t;
#endif

typedef ptr_t pptr_t;
typedef size_t page_t;
