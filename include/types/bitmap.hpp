#pragma once

#include <cstddef>
#include <functional>

namespace types {

class bitmap {
public:
    using deleter_type = std::function<void(unsigned char*, std::size_t)>;

private:
    deleter_type m_del;
    std::size_t m_size;
    unsigned char* m_bm;

    static constexpr std::size_t SZ = sizeof(unsigned char) * 8;

public:
    constexpr bitmap(const deleter_type& del, unsigned char* bm, std::size_t size)
        : m_del(del), m_size(size), m_bm(bm) {}
    constexpr bitmap(deleter_type&& del, unsigned char* bm, std::size_t size)
        : m_del(std::move(del)), m_size(size), m_bm(bm) {}

    explicit constexpr bitmap(std::size_t size)
        : m_del { [](unsigned char* bm, std::size_t) {
            delete[] bm;
        } }
        , m_size { (size / SZ) + ((size % SZ) ? 1 : 0) }
        , m_bm { new unsigned char[m_size] {} }
    { }

    bitmap(const bitmap&) = delete;
    
    constexpr ~bitmap()
    { m_del(m_bm, m_size); }
    
    constexpr bool test(std::size_t n) const
    { return (m_bm[n / SZ] & (1 << (n % SZ))) != 0; }

    constexpr void set(std::size_t n)
    { m_bm[n / SZ] |= (1 << (n % SZ)); }

    constexpr void clear(std::size_t n)
    { m_bm[n / SZ] &= (~(1 << (n % SZ))); }

    constexpr std::size_t size() const noexcept
    { return m_size; }
};

} // namespace types
