#include <types/types.h>

#include <kernel/hw/port.hpp>
#include <kernel/hw/timer.hpp>

constexpr kernel::hw::p8 port_control(0x43);
constexpr kernel::hw::p8 port_count(0x40);

static std::size_t _current_ticks = 0;

SECTION(".text.kinit")
void kernel::hw::timer::init_pit(void)
{
    // set interval
    port_control = 0x34;

    // send interval number
    // 0x2e9a = 11930 = 100Hz
    port_count = 0x9a;
    port_count = 0x2e;
}

void kernel::hw::timer::inc_tick(void)
{
    ++_current_ticks;
}

size_t kernel::hw::timer::current_ticks(void)
{
    return _current_ticks;
}
