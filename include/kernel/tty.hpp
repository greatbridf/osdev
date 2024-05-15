#pragma once

#include <stdint.h>
#include <sys/types.h>
#include <termios.h>

#include <kernel/event/evtqueue.hpp>
#include <types/allocator.hpp>
#include <types/buffer.hpp>
#include <types/cplusplus.hpp>

class tty : public types::non_copyable {
public:
    static constexpr size_t BUFFER_SIZE = 4096;
    static constexpr size_t NAME_SIZE = 32;

private:
    void _real_commit_char(int c);
    void _echo_char(int c);

    int _do_erase(bool should_echo);

public:
    tty();
    virtual void putchar(char c) = 0;
    void print(const char* str);
    size_t read(char* buf, size_t buf_size, size_t n);

    // characters committed to buffer will be handled
    // by the input line discipline (N_TTY)
    void commit_char(int c);

    // print character to the output
    // characters will be handled by the output line discipline
    void show_char(int c);

    void clear_read_buf(void);

    constexpr void set_pgrp(pid_t pgid)
    {
        fg_pgroup = pgid;
    }

    constexpr pid_t get_pgrp(void) const
    {
        return fg_pgroup;
    }

    char name[NAME_SIZE];
    termios termio;

protected:
    types::buffer buf;
    kernel::cond_var m_cv;

    pid_t fg_pgroup;
};

class vga_tty : public virtual tty {
public:
    vga_tty();
    virtual void putchar(char c) override;
};

class serial_tty : public virtual tty {
public:
    serial_tty(int id);
    virtual void putchar(char c) override;

public:
    uint16_t id;
};

inline tty* console;
