#pragma once

#include <cstddef>
#include <string>

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

} // namespace types
