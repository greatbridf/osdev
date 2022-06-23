#pragma once

#include <types/types.h>

#ifdef __cplusplus
extern "C" {
#endif

extern uint32_t* _get_errno(void);

#define errno (*_get_errno())

#ifdef __cplusplus
}
#endif

#define ENOMEM (1 << 0)
#define EEXIST (1 << 1)
#define ENOENT (1 << 2)
#define EINVAL (1 << 3)
#define EISDIR (1 << 4)
#define ENOTDIR (1 << 5)
#define ENOTFOUND (1 << 6)
