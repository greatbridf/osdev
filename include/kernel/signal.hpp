#pragma once

#include <list>
#include <map>

#include <signal.h>
#include <stdint.h>

#include <types/cplusplus.hpp>
#include <types/types.h>

#include <kernel/async/lock.hpp>
#include <kernel/interrupt.hpp>

namespace kernel {

using sigmask_type = uint64_t;

struct sigaction {
    sighandler_t sa_handler;
    unsigned long sa_flags;
    sigrestorer_t sa_restorer;
    sigmask_type sa_mask;
};

class signal_list {
   public:
    using signo_type = uint32_t;
    using list_type = std::list<signo_type>;

   private:
    list_type m_list;
    sigmask_type m_mask{};
    std::map<signo_type, sigaction> m_handlers;
    async::mutex m_mtx;

   public:
    static constexpr bool check_valid(signo_type sig) {
        return sig >= 1 && sig <= 64;
    }

   public:
    constexpr signal_list() = default;
    constexpr signal_list(const signal_list& val)
        : m_list{val.m_list}
        , m_mask{val.m_mask}
        , m_handlers{val.m_handlers}
        , m_mtx{} {}

    constexpr signal_list(signal_list&& val)
        : m_list{std::move(val.m_list)}
        , m_mask{std::move(val.m_mask)}
        , m_handlers{std::move(val.m_handlers)}
        , m_mtx{} {}

    void on_exec();

    sigmask_type get_mask() const;
    void set_mask(sigmask_type mask);
    void mask(sigmask_type mask);
    void unmask(sigmask_type mask);

    void set_handler(signo_type signal, const sigaction& action);
    void get_handler(signo_type signal, sigaction& action) const;

    signo_type pending_signal();

    // return value: whether the thread should wake up
    bool raise(signo_type signal);
    void handle(interrupt_stack* context, mmx_registers* mmxregs);
    void after_signal(signo_type signal);
};

} // namespace kernel
