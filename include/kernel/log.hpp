#pragma once

#include <kernel/tty.hpp>

inline void kmsg(const char* msg)
{
    console->print(msg);
}
