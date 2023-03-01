#pragma once
#include <types/allocator.hpp>
#include <types/buffer.hpp>
#include <types/cplusplus.hpp>
#include <types/stdint.h>

class tty : public types::non_copyable {
public:
    static constexpr size_t BUFFER_SIZE = 4096;
    static constexpr size_t NAME_SIZE = 32;

public:
    tty();
    virtual void putchar(char c) = 0;
    virtual void recvchar(char c) = 0;
    void print(const char* str);

    char name[NAME_SIZE];
    types::buffer<types::kernel_ident_allocator> buf;
    bool echo = true;
};

class vga_tty : public virtual tty {
public:
    vga_tty();
    virtual void putchar(char c) override;
    virtual void recvchar(char c) override;
};

class serial_tty : public virtual tty {
public:
    serial_tty(int id);
    virtual void putchar(char c) override;
    virtual void recvchar(char c) override;

public:
    uint16_t id;
};

inline tty* console;
