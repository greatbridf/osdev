#pragma once
#include <asm/port_io.h>
#include <types/stdint.h>

#define STRUCT_TTY_NAME_LEN (32)

#define SERIAL_TTY_BUFFER_SIZE (4096)

#ifdef __cplusplus
extern "C" {
#endif

struct tty;

struct tty_operations {
    void (*put_char)(struct tty* p_tty, char c);
};

struct tty {
    char name[STRUCT_TTY_NAME_LEN];
    struct tty_operations* ops;
    union {
        uint8_t u8[12];
        uint16_t u16[6];
        uint32_t u32[3];
        void* p[12 / sizeof(void*)];
    } data;
};

// in kernel_main.c
extern struct tty* console;

void tty_print(struct tty* p_tty, const char* str);

int make_serial_tty(struct tty* p_tty, int id, int buffered);
int make_vga_tty(struct tty* p_tty);

#ifdef __cplusplus
}
#endif
