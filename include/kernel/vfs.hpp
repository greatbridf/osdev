#pragma once

#include <bits/alltypes.h>
#include <stdint.h>
#include <sys/stat.h>
#include <sys/types.h>

#include <types/path.hpp>

#include <kernel/mem/paging.hpp>
#include <kernel/vfs/dentry.hpp>

#define NODE_MAJOR(node) (((node) >> 8) & 0xFFU)
#define NODE_MINOR(node) ((node) & 0xFFU)

namespace fs {

constexpr dev_t make_device(uint32_t major, uint32_t minor) {
    return ((major << 8) & 0xFF00U) | (minor & 0xFFU);
}

// buf, buf_size, cnt
using chrdev_read = std::function<ssize_t(char*, std::size_t, std::size_t)>;

// buf, cnt
using chrdev_write = std::function<ssize_t(const char*, std::size_t)>;

struct chrdev_ops {
    chrdev_read read;
    chrdev_write write;
};

int register_char_device(dev_t node, const chrdev_ops& ops);
ssize_t char_device_read(dev_t node, char* buf, size_t buf_size, size_t n);
ssize_t char_device_write(dev_t node, const char* buf, size_t n);

class rust_file_array {
   public:
    struct handle;

   private:
    struct handle* m_handle;

   public:
    rust_file_array(struct handle* handle);
    rust_file_array(const rust_file_array&) = delete;
    ~rust_file_array();

    constexpr rust_file_array(rust_file_array&& other) noexcept
        : m_handle(std::exchange(other.m_handle, nullptr)) {}

    struct handle* get() const;
    void drop();
};

class rust_fs_context {
   public:
    struct handle;

   private:
    struct handle* m_handle;

   public:
    rust_fs_context(struct handle* handle);
    rust_fs_context(const rust_fs_context&) = delete;
    ~rust_fs_context();

    constexpr rust_fs_context(rust_fs_context&& other) noexcept
        : m_handle(std::exchange(other.m_handle, nullptr)) {}

    struct handle* get() const;
    void drop();
};

extern "C" size_t fs_read(struct dentry* file, char* buf, size_t buf_size, size_t offset, size_t n);

} // namespace fs
