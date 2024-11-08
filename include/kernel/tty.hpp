#pragma once

#include <string>

#include <stdint.h>
#include <sys/types.h>
#include <termios.h>

#include <types/allocator.hpp>
#include <types/buffer.hpp>
#include <types/cplusplus.hpp>

#include <kernel/async/lock.hpp>
#include <kernel/async/waitlist.hpp>

namespace kernel::tty {

class tty : public types::non_copyable {
   public:
    static constexpr size_t BUFFER_SIZE = 4096;

   private:
    void _real_commit_char(int c);
    void _echo_char(int c);

    int _do_erase(bool should_echo);

   public:
    explicit tty(std::string name);
    virtual void putchar(char c) = 0;
    void print(const char* str);
    ssize_t read(char* buf, size_t buf_size, size_t n);
    ssize_t write(const char* buf, size_t n);

    // characters committed to buffer will be handled
    // by the input line discipline (N_TTY)
    void commit_char(int c);

    // print character to the output
    // characters will be handled by the output line discipline
    void show_char(int c);

    void clear_read_buf(void);

    // TODO: formal poll support
    int poll();

    int ioctl(int request, unsigned long arg3);

    constexpr void set_pgrp(pid_t pgid) { fg_pgroup = pgid; }

    constexpr pid_t get_pgrp(void) const { return fg_pgroup; }

    termios termio;
    std::string name;

   protected:
    async::mutex mtx_buf;
    types::buffer buf;
    async::wait_list waitlist;

    pid_t fg_pgroup;
};

class vga_tty : public virtual tty {
   public:
    vga_tty();
    virtual void putchar(char c) override;
};

inline tty* console;

int register_tty(tty* tty_dev);

} // namespace kernel::tty
