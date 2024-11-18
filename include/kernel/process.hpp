#pragma once

#include <list>
#include <map>
#include <set>
#include <tuple>
#include <utility>

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
#include <kernel/mem/mm_list.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/vfs.hpp>
#include <kernel/vfs/dentry.hpp>

void NORETURN init_scheduler(kernel::mem::paging::pfn_t kernel_stack_pfn);
/// @return true if returned normally, false if being interrupted
void NORETURN schedule_noreturn(void);

void NORETURN freeze(void);
void NORETURN kill_current(int signo);
