#pragma once

#include <cstddef>
#include <string>
#include <vector>

namespace types {

class string_view {
   private:
    const char* m_str;
    std::size_t m_len;

   public:
    constexpr string_view() : m_str(nullptr), m_len(0) {}
    constexpr string_view(const char* str, std::size_t len)
        : m_str(str), m_len(len) {}
    constexpr string_view(const std::string& str)
        : m_str(str.c_str()), m_len(str.size()) {}
    inline string_view(const char* str)
        : m_str(str), m_len(std::char_traits<char>::length(str)) {}

    constexpr const char* data() const { return m_str; }
    constexpr std::size_t size() const { return m_len; }
    constexpr bool empty() const { return m_len == 0; }

    constexpr const char* begin() const { return m_str; }
    constexpr const char* end() const { return m_str + m_len; }

    constexpr char operator[](std::size_t pos) const { return m_str[pos]; }

    constexpr bool operator==(const string_view& val) const {
        if (m_len != val.m_len)
            return false;
        for (std::size_t i = 0; i < m_len; ++i) {
            if (m_str[i] != val.m_str[i])
                return false;
        }
        return true;
    }

    constexpr bool operator==(const char* str) const {
        for (std::size_t i = 0; i < m_len; ++i) {
            if (m_str[i] != str[i])
                return false;
        }
        return str[m_len] == '\0';
    }

    constexpr bool operator==(const std::string& str) const {
        if (m_len != str.size())
            return false;
        return operator==(str.c_str());
    }

    constexpr bool operator<(const string_view& val) const {
        for (std::size_t i = 0; i < m_len && i < val.m_len; ++i) {
            if (m_str[i] < val.m_str[i])
                return true;
            if (m_str[i] > val.m_str[i])
                return false;
        }
        return m_len < val.m_len;
    }
};

class path_iterator {
   private:
    string_view m_all;
    unsigned m_curlen = 0;
    int m_is_absolute;

   public:
    constexpr path_iterator() = default;
    constexpr path_iterator(string_view str) : m_all{str} {
        m_is_absolute = !m_all.empty() && m_all[0] == '/';
        this->operator++();
    }

    constexpr path_iterator(const std::string& str)
        : path_iterator{string_view{str}} {}
    inline path_iterator(const char* str) : path_iterator{string_view{str}} {}

    constexpr operator bool() const { return !m_all.empty(); }
    constexpr bool is_absolute() const { return m_is_absolute; }

    constexpr string_view operator*() const {
        return string_view{m_all.data(), m_curlen};
    }

    constexpr path_iterator& operator++() {
        std::size_t start = m_curlen;
        while (start < m_all.size() && m_all[start] == '/')
            ++start;

        m_all = string_view{m_all.data() + start, m_all.size() - start};
        if (m_all.empty())
            return *this;

        m_curlen = 0;
        while (m_curlen < m_all.size() && m_all[m_curlen] != '/')
            ++m_curlen;

        return *this;
    }
};

} // namespace types
