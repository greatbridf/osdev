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
    string& append(string&& str)
    {
        return this->append(str.data());
    }
    string& operator+=(const char c)
    {
        this->pop_back();
        this->push_back(c);
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
    string& operator+=(string&& str)
    {
        return this->append(move(str));
    }
    bool operator==(const string& rhs) const
    {
        return strcmp(c_str(), rhs.c_str()) == 0;
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
    char pop(void)
    {
        this->pop_back();
        auto iter = inner_vector_type::back();
        char c = *iter;
        *iter = 0x00;
        return c;
    }
    typename inner_vector_type::iterator_type back(void)
    {
        // TODO: assert
        if (this->empty())
            return typename inner_vector_type::iterator_type((void*)0xffffffff);
        return --inner_vector_type::back();
    }
    typename inner_vector_type::const_iterator_type back(void) const
    {
        // TODO: assert
        if (this->empty())
            return typename inner_vector_type::iterator_type((void*)0xffffffff);
        return --inner_vector_type::back();
    }
    typename inner_vector_type::const_iterator_type cback(void) const
    {
        // TODO: assert
        if (this->empty())
            return typename inner_vector_type::iterator_type((void*)0xffffffff);
        return --inner_vector_type::cback();
    }
};
} // namespace types

#endif