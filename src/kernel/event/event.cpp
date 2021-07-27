#include "kernel/vga.h"
#include <asm/port_io.h>
#include <kernel/event/event.h>
#include <kernel/input/input_event.h>
#include <kernel/stdio.h>
#include <types/list.hpp>

static ::types::list<::input_event> _input_event_queue {};

namespace event {
::types::list<::input_event>& input_event_queue(void)
{
    return _input_event_queue;
}
} // namespace event

void commit_input_event(struct input_event* evt)
{
    event::input_event_queue().push_back(*evt);
}

void dispatch_event(void)
{
    char buf[1024];
    auto& input_event_queue = event::input_event_queue();

    while (!input_event_queue.empty()) {
        for (auto iter = input_event_queue.begin(); iter != input_event_queue.end(); ++iter) {
            const auto& item = *iter;
            snprintf(buf, 1024, "input event: type%x, data%x, code%x\n", item.type, item.data, item.code);
            vga_printk(buf, 0x0fu);
            input_event_queue.erase(iter);
        }
    }
}
