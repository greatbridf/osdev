#pragma once

#include "stdint.h"

#ifdef __GNUC__
#define PACKED __attribute__((__packed__))
#else
#error "no definition for ((PACKED))"
#endif

typedef size_t ptr_t;
typedef ssize_t diff_t;

typedef ptr_t pptr_t;
typedef ssize_t page_t;
