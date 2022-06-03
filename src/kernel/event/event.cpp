#include <kernel/tty.h>
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

    char* ptr = (char*)0x8000000;
    *ptr = 0xff;

    while (!input_event_queue.empty()) {
        for (auto iter = input_event_queue.begin(); iter != input_event_queue.end(); ++iter) {
            const auto& item = *iter;
            snprintf(buf, 1024, "\rinput event: type%x, data%x, code%x\r", item.type, item.data, item.code);
            tty_print(console, buf);
            input_event_queue.erase(iter);
        }
    }
}
