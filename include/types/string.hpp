#pragma once

#include <kernel/stdio.h>
#include <types/allocator.hpp>
#include <types/types.h>
#include <types/vector.hpp>

#ifdef __cplusplus

namespace types {
template <template <typename _value_type> class Allocator = kernel_allocator>
class string : public types::vector<char, Allocator> {
public:
    using inner_vector_type = types::vector<char, Allocator>;
    using size_type = typename inner_vector_type::size_type;

    static inline constexpr size_type npos = (-1U);

public:
    explicit string(size_type capacity = 8)
        : inner_vector_type(capacity)
    {
        this->push_back(0x00);
    }
    string(const char* str, size_type n = npos)
        : string()
    {
        this->append(str, n);
    }
    string(const string& str)
        : inner_vector_type((const inner_vector_type&)str)
    {
    }
    string& append(const char* str, size_type n = npos)
    {
        this->pop_back();

        while (n-- && *str != 0x00) {
            this->push_back(*str);
            ++str;
        }

        this->push_back(0x00);
        return *this;
    }
    string& append(const string& str)
    {
        return this->append(str.data());
    }
    string& operator+=(const char c)
    {
        *this->back() = c;
        this->push_back(0x00);
        return *this;
    }
    string& operator+=(const char* str)
    {
        return this->append(str);
    }
    string& operator+=(const string& str)
    {
        return this->append(str);
    }
    string substr(size_type pos, size_type n = npos)
    {
        return string(this->m_arr + pos, n);
    }
    const char* c_str(void) const noexcept
    {
        return this->data();
    }
    void clear(void)
    {
        inner_vector_type::clear();
        this->push_back(0x00);
    }
};
} // namespace types

#endif
