#pragma once

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

#ifdef __GNUC__
#define likely(expr) (__builtin_expect(!!(expr), 1))
#define unlikely(expr) (__builtin_expect(!!(expr), 0))
#else
#define likely(expr) (!!(expr))
#define unlikely(expr) (!!(expr))
#endif

#ifdef __cplusplus
#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#endif
