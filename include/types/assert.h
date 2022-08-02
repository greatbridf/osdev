#pragma once

#include "types.h"

#ifdef __cplusplus
extern "C" {
#endif

void crash(void);
void _debugger_breakpoint(void);

#ifdef __cplusplus
}
#endif

#ifndef NDEBUG
#define breakpoint() _debugger_breakpoint()
#else
#define breakpoint() _crash()
#endif

#ifndef NDEBUG

#define assert(_statement) \
    if (!(_statement)) {   \
        breakpoint();      \
        crash();           \
    }

#define assert_likely(_statement)  \
    if (unlikely(!(_statement))) { \
        breakpoint();              \
        crash();                   \
    }

#else

#define assert(_statement) ;

#endif
