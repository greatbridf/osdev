#pragma once

#include "types.h"

#ifndef NDEBUG

#define assert(_statement) \
    if (!(_statement))     \
    asm volatile("ud2")

#define assert_likely(_statement) \
    if (unlikely(!(_statement)))  \
    asm volatile("ud2")

#else

#define assert(_statement) ;

#endif
