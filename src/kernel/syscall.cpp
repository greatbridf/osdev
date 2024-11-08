#include <assert.h>
#include <bits/alltypes.h>
#include <errno.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/prctl.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/uio.h>
#include <sys/utsname.h>
#include <sys/wait.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

#include <types/allocator.hpp>
#include <types/elf.hpp>
#include <types/path.hpp>
#include <types/types.h>
#include <types/user_types.hpp>

#include <kernel/async/lock.hpp>
#include <kernel/hw/timer.hpp>
#include <kernel/interrupt.hpp>
#include <kernel/log.hpp>
#include <kernel/process.hpp>
#include <kernel/signal.hpp>
#include <kernel/syscall.hpp>
#include <kernel/task/readyqueue.hpp>
#include <kernel/task/thread.hpp>
#include <kernel/tty.hpp>
#include <kernel/user/thread_local.hpp>
#include <kernel/utsname.hpp>
#include <kernel/vfs.hpp>

#define SYSCALL_HANDLERS_SIZE (404)

#define NOT_IMPLEMENTED not_implemented(__FILE__, __LINE__)

#define SYSCALL32_ARG1(type, name) type name = (type)((data)->regs.rbx)
#define SYSCALL32_ARG2(type, name) type name = (type)((data)->regs.rcx)
#define SYSCALL32_ARG3(type, name) type name = (type)((data)->regs.rdx)
#define SYSCALL32_ARG4(type, name) type name = (type)((data)->regs.rsi)
#define SYSCALL32_ARG5(type, name) type name = (type)((data)->regs.rdi)
#define SYSCALL32_ARG6(type, name) type name = (type)((data)->regs.rbp)

#define _DEFINE_SYSCALL32_ARGS1(type, name, ...) \
    SYSCALL32_ARG1(type, name);                  \
    __VA_OPT__(_DEFINE_SYSCALL32_ARGS2(__VA_ARGS__))

#define _DEFINE_SYSCALL32_ARGS2(type, name, ...) \
    SYSCALL32_ARG2(type, name);                  \
    __VA_OPT__(_DEFINE_SYSCALL32_ARGS3(__VA_ARGS__))

#define _DEFINE_SYSCALL32_ARGS3(type, name, ...) \
    SYSCALL32_ARG3(type, name);                  \
    __VA_OPT__(_DEFINE_SYSCALL32_ARGS4(__VA_ARGS__))

#define _DEFINE_SYSCALL32_ARGS4(type, name, ...) \
    SYSCALL32_ARG4(type, name);                  \
    __VA_OPT__(_DEFINE_SYSCALL32_ARGS5(__VA_ARGS__))

#define _DEFINE_SYSCALL32_ARGS5(type, name, ...) \
    SYSCALL32_ARG5(type, name);                  \
    __VA_OPT__(_DEFINE_SYSCALL32_ARGS6(__VA_ARGS__))

#define _DEFINE_SYSCALL32_ARGS6(type, name, ...) SYSCALL32_ARG6(type, name);

#define _DEFINE_SYSCALL32_END_PARAMS1(type, name, ...) \
    name __VA_OPT__(, _DEFINE_SYSCALL32_END_PARAMS2(__VA_ARGS__))
#define _DEFINE_SYSCALL32_END_PARAMS2(type, name, ...) \
    name __VA_OPT__(, _DEFINE_SYSCALL32_END_PARAMS3(__VA_ARGS__))
#define _DEFINE_SYSCALL32_END_PARAMS3(type, name, ...) \
    name __VA_OPT__(, _DEFINE_SYSCALL32_END_PARAMS4(__VA_ARGS__))
#define _DEFINE_SYSCALL32_END_PARAMS4(type, name, ...) \
    name __VA_OPT__(, _DEFINE_SYSCALL32_END_PARAMS5(__VA_ARGS__))
#define _DEFINE_SYSCALL32_END_PARAMS5(type, name, ...) \
    name __VA_OPT__(, _DEFINE_SYSCALL32_END_PARAMS6(__VA_ARGS__))
#define _DEFINE_SYSCALL32_END_PARAMS6(type, name, ...) name __VA_OPT__(, void)

#define _DEFINE_SYSCALL32_END(name, ...) \
    kernel::syscall::do_##name(__VA_OPT__(_DEFINE_SYSCALL32_END_PARAMS1(__VA_ARGS__)))

#define DEFINE_SYSCALL32_TO(sname, to, ...)                                              \
    static uint32_t _syscall32_##sname(interrupt_stack* data, mmx_registers* mmxregs) {  \
        (void)data, (void)mmxregs;                                                       \
        __VA_OPT__(_DEFINE_SYSCALL32_ARGS1(__VA_ARGS__);)                                \
        return (uint32_t)(uintptr_t)_DEFINE_SYSCALL32_END(to __VA_OPT__(, __VA_ARGS__)); \
    }

#define DEFINE_SYSCALL32(name, ...) DEFINE_SYSCALL32_TO(name, name __VA_OPT__(, ) __VA_ARGS__)

#define DEFINE_SYSCALL32_NORETURN(name, ...)                                 \
    [[noreturn]] static uint32_t _syscall32_##name(interrupt_stack* data,    \
                                                   mmx_registers* mmxregs) { \
        (void)data, (void)mmxregs;                                           \
        __VA_OPT__(_DEFINE_SYSCALL32_ARGS1(__VA_ARGS__);)                    \
        _DEFINE_SYSCALL32_END(name, __VA_ARGS__);                            \
    }

struct syscall_handler_t {
    uint32_t (*handler)(interrupt_stack*, mmx_registers*);
    const char* name;
};

static syscall_handler_t syscall_handlers[SYSCALL_HANDLERS_SIZE];

static inline void not_implemented(const char* pos, int line) {
    kmsgf(
        "[kernel] the function at %s:%d is not implemented, killing the "
        "pid%d...",
        pos, line, current_process->pid);
    current_thread->send_signal(SIGSYS);
}

DEFINE_SYSCALL32(munmap, uintptr_t, addr, size_t, len)
DEFINE_SYSCALL32(poll, pollfd __user*, fds, nfds_t, nfds, int, timeout)
DEFINE_SYSCALL32(socket, int, domain, int, type, int, protocol)

DEFINE_SYSCALL32(mmap_pgoff, uintptr_t, addr, size_t, len, int, prot, int, flags, int, fd, off_t,
                 pgoffset)

DEFINE_SYSCALL32(waitpid, pid_t, waitpid, int __user*, arg1, int, options)
DEFINE_SYSCALL32(getsid, pid_t, pid)
DEFINE_SYSCALL32(setsid)
DEFINE_SYSCALL32(getpgid, pid_t, pid)
DEFINE_SYSCALL32(setpgid, pid_t, pid, pid_t, pgid)
DEFINE_SYSCALL32(getpid)
DEFINE_SYSCALL32(getppid)
DEFINE_SYSCALL32(getuid)
DEFINE_SYSCALL32(geteuid)
DEFINE_SYSCALL32_TO(geteuid32, geteuid)
DEFINE_SYSCALL32(getgid)
DEFINE_SYSCALL32_TO(getgid32, getgid)
DEFINE_SYSCALL32(gettid)
DEFINE_SYSCALL32(set_thread_area, kernel::user::user_desc __user*, ptr)
DEFINE_SYSCALL32(set_tid_address, int __user*, tidptr)
DEFINE_SYSCALL32(prctl, int, option, uintptr_t, arg2)
DEFINE_SYSCALL32(arch_prctl, int, option, uintptr_t, arg2)
DEFINE_SYSCALL32(brk, uintptr_t, addr)
DEFINE_SYSCALL32(kill, pid_t, pid, int, sig)
DEFINE_SYSCALL32(tkill, pid_t, tid, int, sig)
DEFINE_SYSCALL32(rt_sigprocmask, int, how, const kernel::sigmask_type __user*, set,
                 kernel::sigmask_type __user*, oldset, size_t, sigsetsize)
DEFINE_SYSCALL32(rt_sigaction, int, signum, const kernel::sigaction __user*, act,
                 kernel::sigaction __user*, oldact, size_t, sigsetsize)
DEFINE_SYSCALL32(newuname, new_utsname __user*, buf)

DEFINE_SYSCALL32_NORETURN(exit, int, status)

DEFINE_SYSCALL32(gettimeofday, timeval __user*, tv, void __user*, tz)
DEFINE_SYSCALL32_TO(clock_gettime64, clock_gettime, clockid_t, clk_id, timespec __user*, tp)

extern "C" void NORETURN ISR_stub_restore();
static uint32_t _syscall32_fork(interrupt_stack* data, mmx_registers* mmxregs) {
    auto& newproc = procs->copy_from(*current_process);
    auto [iter_newthd, inserted] = newproc.thds.emplace(*current_thread, newproc.pid);
    assert(inserted);
    auto* newthd = &*iter_newthd;

    auto newthd_prev_sp = newthd->kstack.sp;
    assert(!(newthd_prev_sp & 0xf));

    newthd->kstack.sp -= sizeof(interrupt_stack);
    memcpy((void*)(newthd->kstack.sp), data, sizeof(interrupt_stack));

    ((interrupt_stack*)(newthd->kstack.sp))->regs.rax = 0; // return value
    auto isr_restore_sp = newthd->kstack.sp;

    newthd->kstack.sp -= sizeof(mmx_registers);
    memcpy((void*)(newthd->kstack.sp), mmxregs, sizeof(mmx_registers));

    // asm_ctx_switch stack
    // return(start) address
    newthd->kstack.pushq((uintptr_t)ISR_stub_restore);
    newthd->kstack.pushq(0);              // flags
    newthd->kstack.pushq(0);              // r15
    newthd->kstack.pushq(0);              // r14
    newthd->kstack.pushq(0);              // r13
    newthd->kstack.pushq(0);              // r12
    newthd->kstack.pushq(0);              // rbp
    newthd->kstack.pushq(isr_restore_sp); // rbx
    newthd->kstack.pushq(0);              // 0 for alignment
    newthd->kstack.pushq(newthd_prev_sp); // previous sp

    kernel::task::dispatcher::enqueue(newthd);
    return newproc.pid;
}

[[noreturn]] static uint32_t _syscall32_exit_group(interrupt_stack* data, mmx_registers* mmxregs) {
    // we implement exit_group as exit for now
    _syscall32_exit(data, mmxregs);
}

static uint32_t _syscall32_wait4(interrupt_stack* data, mmx_registers* mmxregs) {
    SYSCALL32_ARG4(void __user*, rusage);

    // TODO: getrusage
    if (rusage)
        return -EINVAL;

    return _syscall32_waitpid(data, mmxregs);
}

void kernel::handle_syscall32(int no, interrupt_stack* data, mmx_registers* mmxregs) {
    if (no >= SYSCALL_HANDLERS_SIZE || !syscall_handlers[no].handler) {
        kmsgf("[kernel] syscall %d(%x) isn't implemented", no, no);
        NOT_IMPLEMENTED;

        if (current_thread->signals.pending_signal())
            current_thread->signals.handle(data, mmxregs);
        return;
    }

    // kmsgf_debug("[kernel:debug] (pid\t%d) %s() => {{", current_process->pid,
    // syscall_handlers[no].name);

    asm volatile("sti");
    data->regs.rax = syscall_handlers[no].handler(data, mmxregs);
    data->regs.r8 = 0;
    data->regs.r9 = 0;
    data->regs.r10 = 0;
    data->regs.r11 = 0;
    data->regs.r12 = 0;
    data->regs.r13 = 0;
    data->regs.r14 = 0;
    data->regs.r15 = 0;

    // kmsgf_debug("[kernel:debug] }} => %x", data->regs.rax);

    if (current_thread->signals.pending_signal())
        current_thread->signals.handle(data, mmxregs);
}

#define REGISTER_SYSCALL_HANDLER(no, _name) register_syscall_handler(no, _syscall32_##_name, #_name)

extern "C" void register_syscall_handler(uint32_t no,
                                         uint32_t (*handler)(interrupt_stack*, mmx_registers*),
                                         const char* name) {
    syscall_handlers[no].handler = handler;
    syscall_handlers[no].name = name;
}

extern "C" void r_register_syscall();

SECTION(".text.kinit")
void kernel::init_syscall_table() {
    REGISTER_SYSCALL_HANDLER(0x01, exit);
    REGISTER_SYSCALL_HANDLER(0x02, fork);
    REGISTER_SYSCALL_HANDLER(0x07, waitpid);
    REGISTER_SYSCALL_HANDLER(0x14, getpid);
    REGISTER_SYSCALL_HANDLER(0x25, kill);
    REGISTER_SYSCALL_HANDLER(0x2d, brk);
    REGISTER_SYSCALL_HANDLER(0x2f, getgid);
    REGISTER_SYSCALL_HANDLER(0x39, setpgid);
    REGISTER_SYSCALL_HANDLER(0x40, getppid);
    REGISTER_SYSCALL_HANDLER(0x42, setsid);
    REGISTER_SYSCALL_HANDLER(0x4e, gettimeofday);
    REGISTER_SYSCALL_HANDLER(0x5b, munmap);
    REGISTER_SYSCALL_HANDLER(0x72, wait4);
    REGISTER_SYSCALL_HANDLER(0x7a, newuname);
    REGISTER_SYSCALL_HANDLER(0x84, getpgid);
    REGISTER_SYSCALL_HANDLER(0x93, getsid);
    REGISTER_SYSCALL_HANDLER(0xa8, poll);
    REGISTER_SYSCALL_HANDLER(0xac, prctl);
    REGISTER_SYSCALL_HANDLER(0xae, rt_sigaction);
    REGISTER_SYSCALL_HANDLER(0xaf, rt_sigprocmask);
    REGISTER_SYSCALL_HANDLER(0xc0, mmap_pgoff);
    REGISTER_SYSCALL_HANDLER(0xc7, getuid);
    REGISTER_SYSCALL_HANDLER(0xc8, getgid32);
    REGISTER_SYSCALL_HANDLER(0xc9, geteuid);
    REGISTER_SYSCALL_HANDLER(0xca, geteuid32);
    REGISTER_SYSCALL_HANDLER(0xe0, gettid);
    REGISTER_SYSCALL_HANDLER(0xee, tkill);
    REGISTER_SYSCALL_HANDLER(0xf3, set_thread_area);
    REGISTER_SYSCALL_HANDLER(0xfc, exit_group);
    REGISTER_SYSCALL_HANDLER(0x102, set_tid_address);
    REGISTER_SYSCALL_HANDLER(0x167, socket);
    REGISTER_SYSCALL_HANDLER(0x180, arch_prctl);
    REGISTER_SYSCALL_HANDLER(0x193, clock_gettime64);

    r_register_syscall();
}
