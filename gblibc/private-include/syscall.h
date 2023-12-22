#ifndef __GBLIBC_SYSCALL_H_
#define __GBLIBC_SYSCALL_H_

#include <stdint.h>

#define SYS_exit (0x01)
#define SYS_fork (0x02)
#define SYS_read (0x03)
#define SYS_write (0x04)
#define SYS_open (0x05)
#define SYS_close (0x06)
#define SYS_waitpid (0x07)
#define SYS_execve (0x0b)
#define SYS_chdir (0x0c)
#define SYS_stat (0x12)
#define SYS_getpid (0x14)
#define SYS_fstat (0x1c)
#define SYS_kill (0x25)
#define SYS_dup (0x29)
#define SYS_pipe (0x2a)
#define SYS_brk (0x2d)
#define SYS_ioctl (0x36)
#define SYS_setpgid (0x39)
#define SYS_dup2 (0x3f)
#define SYS_umask (0x3c)
#define SYS_getppid (0x40)
#define SYS_setsid (0x42)
#define SYS_gettimeofday (0x4e)
#define SYS_getdents (0x84)
#define SYS_writev (0x92)
#define SYS_getsid (0x93)
#define SYS_nanosleep (0xa2)
#define SYS_getcwd (0xb7)
#define SYS_set_thread_area (0xf3)
#define SYS_exit_group (0xfc)
#define SYS_set_tid_address (0x102)

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
        "movl %1, %%ebx\n"
        "movl %2, %%eax\n"
        "int $0x80\n"
        "movl %%eax, %0"
        : "=g"(no)
        : "g"(arg), "g"(no)
        : "eax", "ebx");
    return no;
}
static inline uint32_t syscall2(uint32_t no, uint32_t arg1, uint32_t arg2)
{
    asm volatile(
        "movl %1, %%ebx\n"
        "movl %2, %%ecx\n"
        "movl %3, %%eax\n"
        "int $0x80\n"
        "movl %%eax, %0"
        : "=g"(no)
        : "g"(arg1), "g"(arg2), "g"(no)
        : "eax", "ebx", "ecx");
    return no;
}
static inline uint32_t syscall3(uint32_t no, uint32_t arg1, uint32_t arg2, uint32_t arg3)
{
    asm volatile(
        "movl %1, %%ebx\n"
        "movl %2, %%ecx\n"
        "movl %3, %%edx\n"
        "movl %4, %%eax\n"
        "int $0x80\n"
        "movl %%eax, %0"
        : "=g"(no)
        : "g"(arg1), "g"(arg2), "g"(arg3), "g"(no)
        : "eax", "ebx", "ecx", "edx");
    return no;
}

#ifdef __cplusplus
}
#endif

#endif
