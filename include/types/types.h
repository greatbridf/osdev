#pragma once

#include "bitmap.h"
#include "buffer.h"
#include "size.h"
#include "status.h"
#include "stdint.h"

#ifdef __GNUC__
#define NORETURN __attribute__((noreturn))
#else
#error "no definition for ((NORETURN))"
#endif

#ifdef __GNUC__
#define SECTION(x) __attribute__((section(x)))
#else
#error "no definition for ((SECTION))"
#endif

#ifdef __cplusplus
#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#endif
