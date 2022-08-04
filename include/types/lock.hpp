#pragma once

#include <types/stdint.h>

inline void spin_lock(uint32_t volatile* lock_addr)
{
    asm volatile(
        "%=:\n\t\
         movl $1, %%eax\n\t\
         xchgl %%eax, (%0)\n\t\
         test $0, %%eax\n\t\
         jne %=b\n\t\
        "
        :
        : "r"(lock_addr)
        : "eax", "memory");
}

inline void spin_unlock(uint32_t volatile* lock_addr)
{
    asm volatile(
        "movl $0, %%eax\n\
         xchgl %%eax, (%0)"
        :
        : "r"(lock_addr)
        : "eax", "memory");
}

namespace types {

struct mutex {
    using mtx_t = volatile uint32_t;

    mtx_t m_lock = 0;

    inline void lock(void)
    {
        spin_lock(&m_lock);
    }

    inline void unlock(void)
    {
        spin_unlock(&m_lock);
    }
};

class lock_guard {
private:
    mutex& m_mtx;

public:
    explicit lock_guard(mutex& mtx)
        : m_mtx(mtx)
    {
        mtx.lock();
    }

    lock_guard(const lock_guard&) = delete;
    lock_guard(lock_guard&&) = delete;

    ~lock_guard()
    {
        m_mtx.unlock();
    }
};

} // namespace types
