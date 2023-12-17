#pragma once

#include <vector>

#include <string.h>
#include <types/allocator.hpp>
#include <types/types.h>

namespace types {

template <typename Allocator = std::allocator<char>>
class string : public std::vector<char, Allocator> {
public:
    using _vector = std::vector<char, Allocator>;
    using size_type = typename _vector::size_type;
    using iterator = typename _vector::iterator;
    using const_iterator = typename _vector::const_iterator;

    static constexpr size_type npos = -1U;

public:
    constexpr string()
        : _vector()
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
        size_type len = strlen(str);
        const char* last = str + (len < n ? len : n);
        this->insert(end(), str, last);
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
    constexpr string& assign(const char* str, size_type n = npos)
    {
        this->clear();
        return this->append(str, n);
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
        _vector::clear();
        this->push_back(0x00);
    }
    constexpr char pop(void)
    {
        this->pop_back();
        auto& ref = _vector::back();
        char c = ref;
        ref = 0x00;
        return c;
    }
    constexpr iterator end() noexcept
    { return --_vector::end(); }
    constexpr const_iterator end() const noexcept
    { return --_vector::cend(); }
    constexpr const_iterator cend() const noexcept
    { return --_vector::cend(); }
    constexpr char back() const noexcept
    { return *--cend(); }
    constexpr size_type size() const noexcept
    { return _vector::size() - 1; }
    constexpr bool empty() const noexcept
    { return _vector::size() == 1; }
};

} // namespace types
