#pragma once

#include <types/types.h>

// TODO: this whole thing needs rewriting

int32_t keyboard_has_data(void);

void process_keyboard_data(void);

#ifdef __cplusplus
extern "C" void handle_keyboard_interrupt(void);
#else
void handle_keyboard_interrupt(void);
#endif
