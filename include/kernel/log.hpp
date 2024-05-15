#pragma once

#include <stdio.h>

#include <kernel/tty.hpp>

#define kmsgf(fmt, ...) \
    if (1) {\
        char buf[512]; \
        snprintf(buf, sizeof(buf), fmt "\n" __VA_OPT__(,) __VA_ARGS__); \
        console->print(buf); \
    }

#define kmsg(msg) console->print(msg)
