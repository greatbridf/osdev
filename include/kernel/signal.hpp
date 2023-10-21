#pragma once

#include <stdint.h>
#include <types/cplusplus.hpp>
#include <types/list.hpp>

namespace kernel {

using sig_t = uint32_t;

constexpr sig_t SIGINT = 1 << 0;
constexpr sig_t SIGQUIT = 1 << 1;
constexpr sig_t SIGSTOP = 1 << 2;
constexpr sig_t SIGPIPE = 1 << 3;

class signal_list {
public:
    using list_type = types::list<sig_t>;

private:
    list_type m_list;
    sig_t m_mask;

public:
    constexpr signal_list(void)
        : m_mask(0)
    {
    }
    constexpr signal_list(const signal_list& val)
        : m_list(val.m_list)
        , m_mask(val.m_mask)
    {
    }

    constexpr signal_list(signal_list&& val)
        : m_list(std::move(val.m_list))
        , m_mask(val.m_mask)
    {
    }

    constexpr bool empty(void) const
    {
        return this->m_list.empty();
    }

    constexpr void set(sig_t signal)
    {
        if (this->m_mask && signal)
            return;

        this->m_list.push_back(signal);
        this->m_mask |= signal;
    }

    constexpr sig_t pop(void)
    {
        if (this->empty())
            return 0;

        auto iter = this->m_list.begin();
        sig_t signal = *iter;
        this->m_list.erase(iter);

        this->m_mask &= ~signal;

        return signal;
    }
};

} // namespace kernel
