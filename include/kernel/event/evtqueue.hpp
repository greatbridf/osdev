#pragma once

#include <types/list.hpp>
#include <types/lock.hpp>

// declaration in kernel/process.hpp
struct thread;

namespace kernel {

struct evt {
    thread* emitter;
    void* data1;
    void* data2;
    void* data3;
};

class evtqueue {
public:
    // TODO: use small object allocator
    using evt_list_type = types::list<evt>;
    using subscriber_list_type = types::list<thread*>;

private:
    types::mutex m_mtx;
    evt_list_type m_evts;
    subscriber_list_type m_subscribers;

public:
    evtqueue(void) = default;
    evtqueue(const evtqueue&) = delete;
    evtqueue(evtqueue&&);

    void push(evt&& event);
    evt&& front();
    const evt* peek(void) const;

    bool empty(void) const;
    void notify(void);

    void subscribe(thread* thd);
    void unsubscribe(thread* thd);
};

} // namespace kernel
