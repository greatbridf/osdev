#pragma once

#include <types/types.h>

extern uint32_t* _get_errno(void);

#define errno (*_get_errno())

#define ENOMEM 0
#define ENOTFOUND 1
