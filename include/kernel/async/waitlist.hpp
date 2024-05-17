#pragma once

#include <set>

#include <kernel/task/forward.hpp>
#include <kernel/async/lock.hpp>

namespace kernel::async {

class wait_list {
private:
    mutex m_mtx;
    std::set<task::thread*> m_subscribers;

    wait_list(const wait_list&) = delete;

public:
    explicit wait_list() = default;

    // @return whether the wait is interrupted
    bool wait(mutex& lck);

    void subscribe();

    void notify_one();
    void notify_all();
};

} // namespace kernel
