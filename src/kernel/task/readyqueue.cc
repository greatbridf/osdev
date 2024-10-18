#include <list>

#include <kernel/async/lock.hpp>
#include <kernel/task/readyqueue.hpp>
#include <kernel/task/thread.hpp>

using namespace kernel::task;
using kernel::async::mutex, kernel::async::lock_guard_irq;

static mutex dispatcher_mtx;
static std::list<thread*> dispatcher_thds;
static thread* idle_task;

void dispatcher::setup_idle(thread* _idle) {
    idle_task = _idle;
}

void dispatcher::enqueue(thread* thd) {
    lock_guard_irq lck(dispatcher_mtx);

    dispatcher_thds.push_back(thd);
}

void dispatcher::dequeue(thread* thd) {
    lock_guard_irq lck(dispatcher_mtx);

    dispatcher_thds.remove(thd);
}

thread* dispatcher::next() {
    lock_guard_irq lck(dispatcher_mtx);

    if (dispatcher_thds.empty()) {
        idle_task->elected_times++;
        return idle_task;
    }

    auto* front = dispatcher_thds.front();

    if (dispatcher_thds.size() != 1) {
        dispatcher_thds.pop_front();
        dispatcher_thds.push_back(front);
    }

    front->elected_times++;
    return front;
}
