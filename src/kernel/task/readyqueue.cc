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

    auto* retval = dispatcher_thds.front();

    dispatcher_thds.pop_front();
    dispatcher_thds.push_back(retval);

    return retval;
}
