#ifndef __GBLIBC_SYSCALL_H_
#define __GBLIBC_SYSCALL_H_

#include <stdint.h>

#define SYS_read (0)
#define SYS_write (1)
#define SYS_open (2)
#define SYS_close (3)
#define SYS_stat (4)
#define SYS_fstat (5)
#define SYS_brk (12)
#define SYS_ioctl (16)
#define SYS_pipe (22)
#define SYS_dup (32)
#define SYS_dup2 (33)
#define SYS_sleep (35)
#define SYS_getpid (39)
#define SYS_fork (57)
#define SYS_execve (59)
#define SYS_exit (60)
#define SYS_wait (61)
#define SYS_kill (62)
#define SYS_getdents (78)
#define SYS_getcwd (79)
#define SYS_chdir (80)
#define SYS_umask (95)
#define SYS_gettimeofday (96)
#define SYS_setpgid (109)
#define SYS_getppid (110)
#define SYS_setsid (112)
#define SYS_getsid (124)

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
