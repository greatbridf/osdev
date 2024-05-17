#include <kernel/async/waitlist.hpp>

#include <assert.h>

#include <kernel/async/lock.hpp>
#include <kernel/process.hpp>
#include <kernel/task/thread.hpp>

using namespace kernel::async;

bool wait_list::wait(mutex& lock)
{
    this->subscribe();

    auto* curthd = current_thread;
    curthd->set_attr(kernel::task::thread::ISLEEP);

    lock.unlock();
    bool has_signals = schedule();
    lock.lock();

    m_subscribers.erase(curthd);
    return !has_signals;
}

void wait_list::subscribe()
{
    lock_guard lck(m_mtx);

    auto* thd = current_thread;

    bool inserted;
    std::tie(std::ignore, inserted) = m_subscribers.insert(thd);

    assert(inserted);
}

void wait_list::notify_one()
{
    lock_guard lck(m_mtx);

    if (m_subscribers.empty())
        return;

    auto iter = m_subscribers.begin();
    (*iter)->set_attr(kernel::task::thread::READY);

    m_subscribers.erase(iter);
}

void wait_list::notify_all()
{
    lock_guard lck(m_mtx);

    if (m_subscribers.empty())
        return;

    for (auto thd : m_subscribers)
        thd->set_attr(kernel::task::thread::READY);

    m_subscribers.clear();
}
