#include <kernel/task/readyqueue.hpp>

#include <list>

#include <types/lock.hpp>

#include <kernel/task/thread.hpp>

using namespace kernel::task;

static types::mutex dispatcher_mtx;
static std::list<thread*> dispatcher_thds;

void dispatcher::enqueue(thread* thd)
{
    types::lock_guard lck(dispatcher_mtx);

    dispatcher_thds.push_back(thd);
}

void dispatcher::dequeue(thread* thd)
{
    types::lock_guard lck(dispatcher_mtx);

    dispatcher_thds.remove(thd);
}

thread* dispatcher::next()
{
    types::lock_guard lck(dispatcher_mtx);

    auto* retval = dispatcher_thds.front();

    dispatcher_thds.pop_front();
    dispatcher_thds.push_back(retval);

    return retval;
}
