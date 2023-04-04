#pragma once

#include <types/cplusplus.hpp>
#include <types/list.hpp>
#include <types/lock.hpp>

// declaration in kernel/process.hpp
struct thread;

namespace kernel {

class cond_var : public types::non_copyable {
private:
    using list_type = types::list<thread*>;

    types::mutex m_mtx;
    list_type m_subscribers;

public:
    cond_var(void) = default;

    constexpr types::mutex& mtx(void)
    {
        return m_mtx;
    }

    /// @param lock should have already been locked
    bool wait(types::mutex& lock);
    void notify(void);
    void notify_all(void);
};

} // namespace kernel
