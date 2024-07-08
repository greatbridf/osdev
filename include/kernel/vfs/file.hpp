#pragma once

#include <errno.h>
#include <fcntl.h>
#include <sys/types.h>

#include <types/types.h>
#include <types/buffer.hpp>

#include <kernel/async/waitlist.hpp>
#include <kernel/async/lock.hpp>
#include <kernel/vfs/dentry.hpp>

namespace fs {

class pipe : public types::non_copyable {
private:
    static constexpr size_t PIPE_SIZE = 4096;
    static constexpr uint32_t READABLE = 1;
    static constexpr uint32_t WRITABLE = 2;

private:
    types::buffer buf;
    uint32_t flags;
    kernel::async::mutex mtx;

    kernel::async::wait_list waitlist_r;
    kernel::async::wait_list waitlist_w;

public:
    pipe();

    void close_read();
    void close_write();

    int write(const char* buf, size_t n);
    int read(char* buf, size_t n);

    constexpr bool is_readable() const
    {
        return flags & READABLE;
    }

    constexpr bool is_writeable() const
    {
        return flags & WRITABLE;
    }

    constexpr bool is_free() const
    {
        return !(flags & (READABLE | WRITABLE));
    }
};

struct file {
    mode_t mode; // stores the file type in the same format as inode::mode
    dentry* parent {};
    struct file_flags {
        uint32_t read : 1;
        uint32_t write : 1;
        uint32_t append : 1;
    } flags {};

    file(mode_t mode, dentry* parent, file_flags flags)
        : mode(mode) , parent(parent), flags(flags) { }

    virtual ~file() = default;

    virtual ssize_t read(char* __user buf, size_t n) = 0;
    virtual ssize_t do_write(const char* __user buf, size_t n) = 0;

    virtual off_t seek(off_t n, int whence)
    { return (void)n, (void)whence, -ESPIPE; }

    ssize_t write(const char* __user buf, size_t n)
    {
        if (!flags.write)
            return -EBADF;

        if (flags.append) {
            seek(0, SEEK_END);
        }

        return do_write(buf, n);
    }

    // regular files should override this method
    virtual int getdents(char* __user buf, size_t cnt)
    { return (void)buf, (void)cnt, -ENOTDIR; }
    virtual int getdents64(char* __user buf, size_t cnt)
    { return (void)buf, (void)cnt, -ENOTDIR; }
};

struct regular_file : public virtual file {
    virtual ~regular_file() = default;
    std::size_t cursor { };
    inode* ind { };

    regular_file(dentry* parent, file_flags flags, size_t cursor, inode* ind);

    virtual ssize_t read(char* __user buf, size_t n) override;
    virtual ssize_t do_write(const char* __user buf, size_t n) override;
    virtual off_t seek(off_t n, int whence) override;
    virtual int getdents(char* __user buf, size_t cnt) override;
    virtual int getdents64(char* __user buf, size_t cnt) override;
};

struct fifo_file : public virtual file {
    virtual ~fifo_file() override;
    std::shared_ptr<pipe> ppipe;

    fifo_file(dentry* parent, file_flags flags, std::shared_ptr<fs::pipe> ppipe);

    virtual ssize_t read(char* __user buf, size_t n) override;
    virtual ssize_t do_write(const char* __user buf, size_t n) override;
};

} // namespace fs
