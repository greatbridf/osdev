#include <bits/alltypes.h>
#include <time.h>

#include <kernel/hw/timer.hpp>
#include <kernel/log.hpp>
#include <kernel/process.hpp>
#include <kernel/syscall.hpp>

#define NOT_IMPLEMENTED not_implemented(__FILE__, __LINE__)

static inline void not_implemented(const char* pos, int line)
{
    kmsgf("[kernel] the function at %s:%d is not implemented, killing the pid%d...",
            pos, line, current_process->pid);
    current_thread->send_signal(SIGSYS);
}

int kernel::syscall::do_clock_gettime(clockid_t clk_id, timespec __user* tp)
{
    if (clk_id != CLOCK_REALTIME && clk_id != CLOCK_MONOTONIC) {
        NOT_IMPLEMENTED;
        return -EINVAL;
    }

    if (!tp)
        return -EFAULT;

    auto time = hw::timer::current_ticks();

    // TODO: copy_to_user
    tp->tv_sec = time / 100;
    tp->tv_nsec = 10000000 * (time % 100);

    return 0;
}

int kernel::syscall::do_gettimeofday(timeval __user* tv, void __user* tz)
{
    // TODO: return time of the day, not time from this boot
    if (tz) [[unlikely]]
        return -EINVAL;

    if (tv) {
        // TODO: use copy_to_user
        auto ticks = kernel::hw::timer::current_ticks();
        tv->tv_sec = ticks / 100;
        tv->tv_usec = ticks * 10 * 1000;
    }

    return 0;
}
