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
#include <types/lock.hpp>

static std::list<::input_event>* _input_event_queue;

namespace event {
std::list<::input_event>& input_event_queue(void)
{
    if (!_input_event_queue) {
        _input_event_queue = new std::list<input_event>;
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

    // char* ptr = (char*)0x8000000;
    // *ptr = 0xff;

    while (!input_event_queue.empty()) {
        for (auto iter = input_event_queue.begin(); iter != input_event_queue.end(); ++iter) {
            const auto& item = *iter;
            snprintf(buf, 1024, "\rinput event: type%x, data%x, code%x\r", item.type, item.data, item.code);
            kmsg(buf);
            input_event_queue.erase(iter);
        }
    }
}

bool kernel::cond_var::wait(types::mutex& lock)
{
    kernel::tasks::thread* thd = current_thread;

    current_thread->sleep();
    m_subscribers.push_back(thd);

    lock.unlock();
    bool ret = schedule();
    lock.lock();

    m_subscribers.remove(thd);
    return ret;
}

void kernel::cond_var::notify(void)
{
    types::lock_guard lck(m_mtx);

    auto iter = m_subscribers.begin();
    if (iter == m_subscribers.end())
        return;

    auto* thd = *iter;
    thd->wakeup();

    m_subscribers.erase(iter);
}

void kernel::cond_var::notify_all(void)
{
    types::lock_guard lck(m_mtx);

    for (auto& thd : m_subscribers)
        thd->wakeup();

    m_subscribers.clear();
}
