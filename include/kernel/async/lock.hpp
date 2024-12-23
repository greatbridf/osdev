#pragma once

#include <cstddef>

#include <stdint.h>

namespace kernel::async {

using spinlock_t = unsigned long volatile;
using lock_context_t = unsigned long;

void init_spinlock(spinlock_t& lock);

void spin_lock(spinlock_t& lock);
void spin_unlock(spinlock_t& lock);

lock_context_t spin_lock_irqsave(spinlock_t& lock);
void spin_unlock_irqrestore(spinlock_t& lock, lock_context_t context);

class mutex {
   private:
    spinlock_t m_lock;

   public:
    constexpr mutex() : m_lock{0} {}
    mutex(const mutex&) = delete;
    ~mutex();

    lock_context_t lock_irq();
    void unlock_irq(lock_context_t state);
};

class lock_guard_irq {
   private:
    mutex& m_mtx;
    lock_context_t state;

   public:
    explicit inline lock_guard_irq(mutex& mtx) : m_mtx{mtx} {
        state = m_mtx.lock_irq();
    }
    lock_guard_irq(const lock_guard_irq&) = delete;

    inline ~lock_guard_irq() { m_mtx.unlock_irq(state); }
};

} // namespace kernel::async
