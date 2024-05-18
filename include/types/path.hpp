#pragma once

#include <cstddef>
#include <string>
#include <vector>

namespace types {

class path {
public:
    using item_string = std::string;
    using item_vector = std::vector<item_string>;
    using string_type = std::string;
    using size_type = std::size_t;
    using iterator = item_vector::const_iterator;

private:
    item_vector m_vec;

public:
    constexpr path() = default;
    constexpr path(const path& val) = default;
    constexpr path(path&& val) = default;
    explicit constexpr path(const char* str, size_type len = -1U)
    { append(str, len); }

    constexpr path& operator=(const path& val) = default;
    constexpr path& operator=(path&& val) = default;
    constexpr path& operator=(const char* str)
    {
        m_vec.clear();
        append(str);
        return *this;
    }

    constexpr string_type full_path() const
    {
        string_type str;
        for (auto iter = m_vec.begin(); iter != m_vec.end(); ++iter) {
            if (iter != m_vec.begin()
                || (m_vec.front().empty() && m_vec.size() == 1))
                str += '/';
            str += *iter;
        }
        return str;
    }
    constexpr item_string last_name() const
    { return m_vec.empty() ? item_string {} : m_vec.back(); }
    constexpr bool empty() const
    { return m_vec.empty(); }

    constexpr bool is_absolute() const { return !empty() && !m_vec[0][0]; }
    constexpr bool is_relative() const { return !empty() && !is_absolute(); }

    constexpr path& append(const char* str, size_type len = -1U)
    {
        const char* start = str;

        if (len && *start == '/')
            clear();

        while (len-- && *str) {
            if (*str == '/') {
                if (m_vec.empty() || str != start)
                    m_vec.emplace_back(start, str - start);
                start = str + 1;
            }
            ++str;
        }
        if (str != start || m_vec.size() != 1 || !m_vec.front().empty())
            m_vec.emplace_back(start, str - start);

        return *this;
    }
    constexpr path& append(const path& val)
    {
        if (&val == this)
            return *this;

        if (val.is_absolute()) {
            *this = val;
            return *this;
        }

        m_vec.insert(m_vec.end(), val.m_vec.begin(), val.m_vec.end());
        return *this;
    }

    constexpr void clear() { m_vec.clear(); }
    constexpr void remove_last()
    {
        if (m_vec.size() > 1)
            m_vec.pop_back();
    }

    constexpr path& operator+=(const char* str)
    { return append(str); }
    constexpr path& operator+=(const path& val)
    { return append(val); }

    constexpr path operator+(const char* str) const
    { return path{*this}.append(str); }
    constexpr path operator+(const path& val)
    { return path{*this}.append(val); }

    constexpr bool operator==(const char* str) const
    {
        return full_path() == str;
    }

    constexpr iterator begin() const { return m_vec.cbegin(); }
    constexpr iterator end() const { return m_vec.cend(); }
};

} // namespace types
