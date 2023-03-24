#include <asm/port_io.h>
#include <assert.h>
#include <kernel/event/event.h>
#include <kernel/event/evtqueue.hpp>
#include <kernel/input/input_event.h>
#include <kernel/log.hpp>
#include <kernel/process.hpp>
#include <stdio.h>
#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/list.hpp>
#include <types/lock.hpp>

static ::types::list<::input_event>* _input_event_queue;

namespace event {
::types::list<::input_event>& input_event_queue(void)
{
    if (!_input_event_queue) {
        _input_event_queue = new types::list<input_event>;
    }
    return *_input_event_queue;
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
            kmsg(buf);
            input_event_queue.erase(iter);
        }
    }
}

kernel::evtqueue::evtqueue(evtqueue&& q)
    : m_evts { types::move(q.m_evts) }
    , m_subscribers { types::move(q.m_subscribers) }
{
}

void kernel::evtqueue::push(kernel::evt&& event)
{
    types::lock_guard lck(m_mtx);
    m_evts.push_back(types::move(event));
    this->notify();
}

kernel::evt kernel::evtqueue::front()
{
    assert(!this->empty());
    types::lock_guard lck(m_mtx);

    auto iter = m_evts.begin();
    evt e = types::move(*iter);
    m_evts.erase(iter);
    return e;
}

const kernel::evt* kernel::evtqueue::peek(void) const
{
    return &m_evts.begin();
}

bool kernel::evtqueue::empty(void) const
{
    return m_evts.empty();
}

void kernel::evtqueue::notify(void)
{
    for (auto* sub : m_subscribers) {
        sub->attr.ready = 1;
        sub->attr.wait = 0;
        readythds->push(sub);
    }
}

void kernel::evtqueue::subscribe(thread* thd)
{
    m_subscribers.push_back(thd);
}

void kernel::evtqueue::unsubscribe(thread* thd)
{
    m_subscribers.erase(m_subscribers.find(thd));
}
