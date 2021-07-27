#pragma once

#include <kernel/input/input_event.h>
#include <types/types.h>

#ifdef __cplusplus
extern "C" {
#endif

void commit_input_event(struct input_event* evt);

void dispatch_event(void);

#ifdef __cplusplus
}
#endif
