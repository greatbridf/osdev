#pragma once
#include <asm/port_io.h>

#define STRUCT_TTY_NAME_LEN (32)

struct tty;

struct tty_operations
{
    void (*put_char)(struct tty* p_tty, char c);
};

struct tty
{
    char name[STRUCT_TTY_NAME_LEN];
    struct tty_operations* ops;
    char data[12];
};

void tty_print(struct tty* p_tty, const char* str);

int make_serial_tty(struct tty* p_tty, int id);
int make_vga_tty(struct tty* p_tty);
