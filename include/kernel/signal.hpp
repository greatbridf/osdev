#pragma once

#include <list>

#include <signal.h>
#include <stdint.h>
#include <types/types.h>

#include <types/cplusplus.hpp>

namespace kernel {

class signal_list {
public:
    using signo_type = uint32_t;
    using list_type = std::list<signo_type>;

private:
    list_type m_list;
    signo_type m_mask;
    sig_t m_handlers[32];

public:
    static constexpr bool check_valid(signo_type sig)
    {
        return sig > 0 && sig < 32;
    }

public:
    signal_list();
    constexpr signal_list(const signal_list& val) = default;
    constexpr signal_list(signal_list&& val) = default;

    void on_exec();

    void get_mask(sigset_t* __user mask) const;
    void set_mask(const sigset_t* __user mask);

    constexpr bool is_masked(signo_type signal) const { return m_mask & (1 << signal); }
    constexpr bool empty(void) const { return m_list.empty(); }

    void set(signo_type signal);
    signo_type handle();
    void after_signal(signo_type signal);
};

} // namespace kernel
