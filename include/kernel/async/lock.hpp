#pragma once

#include <stdint.h>

namespace kernel::async {

using spinlock_t = uint32_t volatile;
using preempt_count_t = size_t;

void preempt_disable();
void preempt_enable();
preempt_count_t preempt_count();

void init_spinlock(spinlock_t& lock);

void spin_lock(spinlock_t& lock);
void spin_unlock(spinlock_t& lock);

uint32_t spin_lock_irqsave(spinlock_t& lock);
void spin_unlock_irqrestore(spinlock_t& lock, uint32_t state);

class mutex {
private:
    spinlock_t m_lock;

public:
    constexpr mutex() : m_lock {0} { }
    mutex(const mutex&) = delete;
    ~mutex();

    void lock();
    void unlock();

    uint32_t lock_irq();
    void unlock_irq(uint32_t state);
};

class lock_guard {
private:
    mutex& m_mtx;

public:
    explicit inline lock_guard(mutex& mtx)
        : m_mtx {mtx} { m_mtx.lock(); }
    lock_guard(const lock_guard&) = delete;

    inline ~lock_guard() { m_mtx.unlock(); }
};

class lock_guard_irq {
private:
    mutex& m_mtx;
    uint32_t state;

public:
    explicit inline lock_guard_irq(mutex& mtx)
        : m_mtx {mtx} { state = m_mtx.lock_irq(); }
    lock_guard_irq(const lock_guard_irq&) = delete;

    inline ~lock_guard_irq() { m_mtx.unlock_irq(state); }
};

} // namespace kernel::async
