#include <asm/port_io.h>
#include <kernel/hw/timer.h>

static size_t _current_ticks = 0;

SECTION(".text.kinit")
void init_pit(void)
{
    // set interval
    asm_outb(PORT_PIT_CONTROL, 0x34);

    // send interval number
    // 0x2e9a = 11930 = 100Hz
    asm_outb(PORT_PIT_COUNT, 0x9a);
    asm_outb(PORT_PIT_COUNT, 0x2e);
}

void inc_tick(void)
{
    ++_current_ticks;
}

size_t current_ticks(void)
{
    return _current_ticks;
}
