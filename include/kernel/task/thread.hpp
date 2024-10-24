#pragma once

#include <cstddef>
#include <string>

#include <stdint.h>
#include <sys/types.h>

#include <types/types.h>

#include <kernel/mem/paging.hpp>
#include <kernel/signal.hpp>
#include <kernel/user/thread_local.hpp>

namespace kernel::task {

using tid_t = std::size_t;

struct thread {
   public:
    using thd_attr_t = uint32_t;
    static constexpr thd_attr_t SYSTEM = 0x01;
    static constexpr thd_attr_t READY = 0x02;
    static constexpr thd_attr_t STOPPED = 0x04;
    static constexpr thd_attr_t ZOMBIE = 0x08;
    static constexpr thd_attr_t ISLEEP = 0x10;
    static constexpr thd_attr_t USLEEP = 0x20;

   private:
    struct kernel_stack {
        mem::paging::pfn_t pfn;
        uintptr_t sp;

        kernel_stack();
        kernel_stack(const kernel_stack& other);
        kernel_stack(kernel_stack&& other);
        ~kernel_stack();

        uint64_t pushq(uint64_t val);
        uint32_t pushl(uint32_t val);

        void load_interrupt_stack() const;
    };

   public:
    kernel_stack kstack;
    pid_t owner;
    thd_attr_t attr;
    signal_list signals;

    int* __user set_child_tid{};
    int* __user clear_child_tid{};

    std::string name{};
    uint64_t tls_desc32{};
    std::size_t elected_times{};

    explicit thread(std::string name, pid_t owner);
    thread(const thread& val, pid_t owner);

    int set_thread_area(user::user_desc* ptr);
    int load_thread_area32() const;

    void set_attr(thd_attr_t new_attr);

    void send_signal(signal_list::signo_type signal);

    thread(thread&& val) = default;

    tid_t tid() const;

    bool operator<(const thread& rhs) const;
    bool operator==(const thread& rhs) const;
};

} // namespace kernel::task
