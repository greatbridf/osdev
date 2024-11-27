#pragma once

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdint.h>
#include <sys/types.h>

#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/path.hpp>
#include <types/types.h>

#include <kernel/interrupt.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/vfs.hpp>

void NORETURN freeze(void);
