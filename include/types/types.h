#pragma once

#include "stdint.h"

#define __user

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
#define PACKED __attribute__((__packed__))
#else
#error "no definition for ((PACKED))"
#endif

#ifdef __GNUC__
#define likely(expr) (__builtin_expect(!!(expr), 1))
#define unlikely(expr) (__builtin_expect(!!(expr), 0))
#else
#define likely(expr) (!!(expr))
#define unlikely(expr) (!!(expr))
#endif

typedef size_t refcount_t;

#ifdef __cplusplus
#include <types/cplusplus.hpp>
#endif
