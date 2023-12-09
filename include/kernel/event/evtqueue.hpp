#pragma once

#include <list>

#include <types/cplusplus.hpp>
#include <types/lock.hpp>

namespace kernel {

namespace tasks {

// declaration in kernel/process.hpp
struct thread;

} // namespace tasks

class cond_var : public types::non_copyable {
private:
    using list_type = std::list<tasks::thread*>;

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
