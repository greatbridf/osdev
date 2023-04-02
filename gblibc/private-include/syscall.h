#ifndef __GBLIBC_SYSCALL_H_
#define __GBLIBC_SYSCALL_H_

#include <stdint.h>

#define SYS_fork (0x00)
#define SYS_write (0x01)
#define SYS_sleep (0x02)
#define SYS_chdir (0x03)
#define SYS_exec (0x04)
#define SYS_exit (0x05)
#define SYS_wait (0x06)
#define SYS_read (0x07)
#define SYS_getdents (0x08)
#define SYS_open (0x09)
#define SYS_getcwd (0x0a)
#define SYS_setsid (0x0b)
#define SYS_getsid (0x0c)
#define SYS_close (0x0d)
#define SYS_dup (0x0e)
#define SYS_dup2 (0x0f)

#ifdef __cplusplus
extern "C" {
#endif

static inline uint32_t syscall0(uint32_t no)
{
    asm volatile(
        "movl %1, %%eax\n"
        "int $0x80\n"
        "movl %%eax, %0"
        : "=g"(no)
        : "g"(no)
        : "eax");
    return no;
}
static inline uint32_t syscall1(uint32_t no, uint32_t arg)
{
    asm volatile(
        "movl %1, %%edi\n"
        "movl %2, %%eax\n"
        "int $0x80\n"
        "movl %%eax, %0"
        : "=g"(no)
        : "g"(arg), "g"(no)
        : "eax", "edi");
    return no;
}
static inline uint32_t syscall2(uint32_t no, uint32_t arg1, uint32_t arg2)
{
    asm volatile(
        "movl %1, %%edi\n"
        "movl %2, %%esi\n"
        "movl %3, %%eax\n"
        "int $0x80\n"
        "movl %%eax, %0"
        : "=g"(no)
        : "g"(arg1), "g"(arg2), "g"(no)
        : "eax", "edi", "esi");
    return no;
}
static inline uint32_t syscall3(uint32_t no, uint32_t arg1, uint32_t arg2, uint32_t arg3)
{
    asm volatile(
        "movl %1, %%edi\n"
        "movl %2, %%esi\n"
        "movl %3, %%edx\n"
        "movl %4, %%eax\n"
        "int $0x80\n"
        "movl %%eax, %0"
        : "=g"(no)
        : "g"(arg1), "g"(arg2), "g"(arg3), "g"(no)
        : "eax", "edx", "edi", "esi");
    return no;
}

#ifdef __cplusplus
}
#endif

#endif
