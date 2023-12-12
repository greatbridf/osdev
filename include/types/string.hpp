#pragma once

#include <vector>

#include <string.h>
#include <types/allocator.hpp>
#include <types/types.h>

namespace types {
template <typename Allocator =
    types::allocator_adapter<char, types::kernel_allocator>>
class string : public std::vector<char, Allocator> {
public:
    using inner_vector_type = std::vector<char, Allocator>;
    using size_type = typename inner_vector_type::size_type;
    using iterator = typename inner_vector_type::iterator;
    using const_iterator = typename inner_vector_type::const_iterator;

    static inline constexpr size_type npos = (-1U);

public:
    constexpr string()
        : inner_vector_type()
    {
        this->reserve(8);
        this->push_back(0x00);
    }
    constexpr string(const char* str, size_type n = npos)
        : string()
    {
        this->append(str, n);
    }
    constexpr string& append(const char* str, size_type n = npos)
    {
        this->pop_back();

        while (n-- && *str != 0x00) {
            this->push_back(*str);
            ++str;
        }

        this->push_back(0x00);
        return *this;
    }
    constexpr string& append(const string& str)
    {
        return this->append(str.data());
    }
    constexpr string& operator+=(const char c)
    {
        this->insert(this->end(), c);
        return *this;
    }
    constexpr string& operator+=(const char* str)
    {
        return this->append(str);
    }
    constexpr string& operator+=(const string& str)
    {
        return this->append(str);
    }
    constexpr bool operator==(const string& rhs) const
    {
        return strcmp(c_str(), rhs.c_str()) == 0;
    }
    constexpr string substr(size_type pos, size_type n = npos)
    {
        return string(this->m_arr + pos, n);
    }
    constexpr const char* c_str(void) const noexcept
    {
        return this->data();
    }
    constexpr void clear()
    {
        inner_vector_type::clear();
        this->push_back(0x00);
    }
    constexpr char pop(void)
    {
        this->pop_back();
        auto& ref = inner_vector_type::back();
        char c = ref;
        ref = 0x00;
        return c;
    }
    constexpr iterator end() noexcept
    { return --inner_vector_type::end(); }
    constexpr const_iterator end() const noexcept
    { return --inner_vector_type::cend(); }
    constexpr const_iterator cend() const noexcept
    { return --inner_vector_type::cend(); }
};
} // namespace types
