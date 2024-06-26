#pragma once

#include <bits/alltypes.h>
#include <poll.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/uio.h>
#include <sys/utsname.h>
#include <time.h>

#include <types/types.h>

#include <kernel/interrupt.hpp>
#include <kernel/signal.hpp>
#include <kernel/user/thread_local.hpp>

#define SYSCALL64_ARG1(type, name) type name = (type)((data)->head.s_regs.rdi)
#define SYSCALL64_ARG2(type, name) type name = (type)((data)->head.s_regs.rsi)
#define SYSCALL64_ARG3(type, name) type name = (type)((data)->head.s_regs.rdx)
#define SYSCALL64_ARG4(type, name) type name = (type)((data)->head.s_regs.r10)
#define SYSCALL64_ARG5(type, name) type name = (type)((data)->head.s_regs.r8)
#define SYSCALL64_ARG6(type, name) type name = (type)((data)->head.s_regs.r9)

namespace kernel {
void init_syscall_table();

void handle_syscall32(int no, interrupt_stack_normal* data, mmx_registers* mmxregs);
void handle_syscall64(int no, interrupt_stack_normal* data, mmx_registers* mmxregs);

namespace syscall {
// in fileops.cc
ssize_t do_write(int fd, const char __user* buf, size_t n);
ssize_t do_read(int fd, char __user* buf, size_t n);
int do_close(int fd);
int do_dup(int old_fd);
int do_dup2(int old_fd, int new_fd);
int do_pipe(int __user* pipefd);
ssize_t do_getdents(int fd, char __user* buf, size_t cnt);
ssize_t do_getdents64(int fd, char __user* buf, size_t cnt);
int do_open(const char __user* path, int flags, mode_t mode);
int do_symlink(const char __user* target, const char __user* linkpath);
int do_readlink(const char __user* pathname, char __user* buf, size_t buf_size);
int do_ioctl(int fd, unsigned long request, uintptr_t arg3);
ssize_t do_readv(int fd, const iovec __user* iov, int iovcnt);
ssize_t do_writev(int fd, const iovec __user* iov, int iovcnt);
off_t do_lseek(int fd, off_t offset, int whence);
uintptr_t do_mmap_pgoff(uintptr_t addr, size_t len,
        int prot, int flags, int fd, off_t pgoffset);
int do_munmap(uintptr_t addr, size_t len);
ssize_t do_sendfile(int out_fd, int in_fd, off_t __user* offset, size_t count);
int do_statx(int dirfd, const char __user* path,
        int flags, unsigned int mask, statx __user* statxbuf);
int do_fcntl(int fd, int cmd, unsigned long arg);
int do_poll(pollfd __user* fds, nfds_t nfds, int timeout);
int do_mknod(const char __user* pathname, mode_t mode, dev_t dev);
int do_access(const char __user* pathname, int mode);
int do_unlink(const char __user* pathname);
int do_truncate(const char __user* pathname, long length);
int do_mkdir(const char __user* pathname, mode_t mode);

// in procops.cc
int do_chdir(const char __user* path);
[[noreturn]] int do_exit(int status);
int do_waitpid(pid_t waitpid, int __user* arg1, int options);
pid_t do_getsid(pid_t pid);
pid_t do_setsid();
pid_t do_getpgid(pid_t pid);
int do_setpgid(pid_t pid, pid_t pgid);
int do_set_thread_area(user::user_desc __user* ptr);
pid_t do_set_tid_address(int __user* tidptr);
int do_prctl(int option, uintptr_t arg2);
int do_arch_prctl(int option, uintptr_t arg2);
pid_t do_getpid();
pid_t do_getppid();
uid_t do_getuid();
uid_t do_geteuid();
gid_t do_getgid();
pid_t do_gettid();
char __user* do_getcwd(char __user* buf, size_t buf_size);
uintptr_t do_brk(uintptr_t addr);
int do_umask(mode_t mask);
int do_kill(pid_t pid, int sig);
int do_rt_sigprocmask(int how, const kernel::sigmask_type __user* set,
        kernel::sigmask_type __user* oldset, size_t sigsetsize);
int do_rt_sigaction(int signum, const sigaction __user* act,
        sigaction __user* oldact, size_t sigsetsize);
int do_newuname(new_utsname __user* buf);

struct execve_retval {
    uintptr_t ip;
    uintptr_t sp;
    int status;
};

execve_retval do_execve(
        const char __user* exec,
        char __user* const __user* argv,
        char __user* const __user* envp);

// in mount.cc
int do_mount(
        const char __user* source,
        const char __user* target,
        const char __user* fstype,
        unsigned long flags,
        const void __user* _fsdata);

// in infoops.cc
int do_clock_gettime(clockid_t clk_id, timespec __user* tp);
int do_gettimeofday(timeval __user* tv, void __user* tz);

} // namespace kernel::syscall

} // namespace kernel
