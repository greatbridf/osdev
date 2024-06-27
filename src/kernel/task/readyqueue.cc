#include <kernel/task/readyqueue.hpp>

#include <list>

#include <kernel/async/lock.hpp>
#include <kernel/task/thread.hpp>

using namespace kernel::task;
using kernel::async::mutex, kernel::async::lock_guard_irq;

static mutex dispatcher_mtx;
static std::list<thread*> dispatcher_thds;

void dispatcher::enqueue(thread* thd)
{
    lock_guard_irq lck(dispatcher_mtx);

    dispatcher_thds.push_back(thd);
}

void dispatcher::dequeue(thread* thd)
{
    lock_guard_irq lck(dispatcher_mtx);

    dispatcher_thds.remove(thd);
}

thread* dispatcher::next()
{
    lock_guard_irq lck(dispatcher_mtx);
    auto back = dispatcher_thds.back();

    if (dispatcher_thds.size() == 1) {
        back->elected_times++;
        return back;
    }

    if (dispatcher_thds.size() == 2) {
        if (back->owner == 0) {
            auto front = dispatcher_thds.front();
            front->elected_times++;
            return front;
        }
        back->elected_times++;
        return back;
    }

    auto* retval = dispatcher_thds.front();

    dispatcher_thds.pop_front();
    dispatcher_thds.push_back(retval);

    retval->elected_times++;
    return retval;
}
