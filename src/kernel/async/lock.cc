#include <assert.h>

#include <kernel/async/lock.hpp>

namespace kernel::async {

static inline void _raw_spin_lock(spinlock_t* lock_addr) {
    asm volatile(
        "%=:\n\t\
         mov $1, %%eax\n\t\
         xchg %%eax, (%0)\n\t\
         cmp $0, %%eax\n\t\
         jne %=b\n\t\
        "
        :
        : "r"(lock_addr)
        : "eax", "memory");
}

static inline void _raw_spin_unlock(spinlock_t* lock_addr) {
    asm volatile(
        "mov $0, %%eax\n\
         xchg %%eax, (%0)"
        :
        : "r"(lock_addr)
        : "eax", "memory");
}

static inline lock_context_t _save_interrupt_state() {
    lock_context_t retval;
    asm volatile(
        "pushf\n\t"
        "pop %0\n\t"
        "cli"
        : "=g"(retval)
        :
        :);

    return retval;
}

static inline void _restore_interrupt_state(lock_context_t context) {
    asm volatile(
        "push %0\n\t"
        "popf"
        :
        : "g"(context)
        :);
}

extern "C" void r_preempt_disable();
extern "C" void r_preempt_enable();
extern "C" unsigned long r_preempt_count();

void preempt_disable() {
    r_preempt_disable();
}

void preempt_enable() {
    r_preempt_enable();
}

unsigned long preempt_count() {
    return r_preempt_count();
}

void spin_lock(spinlock_t& lock) {
    preempt_disable();
    _raw_spin_lock(&lock);
}

void spin_unlock(spinlock_t& lock) {
    _raw_spin_unlock(&lock);
    preempt_enable();
}

lock_context_t spin_lock_irqsave(spinlock_t& lock) {
    auto state = _save_interrupt_state();
    preempt_disable();

    _raw_spin_lock(&lock);

    return state;
}

void spin_unlock_irqrestore(spinlock_t& lock, lock_context_t state) {
    _raw_spin_unlock(&lock);
    preempt_enable();
    _restore_interrupt_state(state);
}

mutex::~mutex() {
    assert(m_lock == 0);
}

lock_context_t mutex::lock_irq() {
    return spin_lock_irqsave(m_lock);
}

void mutex::unlock_irq(lock_context_t state) {
    spin_unlock_irqrestore(m_lock, state);
}

} // namespace kernel::async
