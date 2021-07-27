#pragma once

#include <types/types.h>

// keyboard event code
#define KEY_DOWN 0
#define KEY_UP 1

struct input_event {
    enum input_event_type {
        KEYBOARD,
    };

    enum input_event_type type;
    uint32_t code;
    uint32_t data;
};
