#pragma once

#include <cstddef>

namespace kernel::hw::timer {
void init_pit(void);
void inc_tick(void);

std::size_t current_ticks(void);

}
