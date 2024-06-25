#pragma once

#include <stdio.h>

#include <kernel/tty.hpp>

#define kmsgf(fmt, ...) \
    if (1) {\
        char buf[512]; \
        snprintf(buf, sizeof(buf), fmt "\n" __VA_OPT__(,) __VA_ARGS__); \
        if (kernel::tty::console) kernel::tty::console->print(buf); \
    }

#define kmsg(msg) if (kernel::tty::console) kernel::tty::console->print(msg "\n")
