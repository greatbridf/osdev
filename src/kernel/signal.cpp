#include <kernel/process.hpp>
#include <kernel/signal.hpp>

#include <signal.h>

using kernel::signal_list;
using signo_type = signal_list::signo_type;

static void continue_process(int) { }

static void stop_process(int)
{
    current_thread->attr.ready = 0;
    current_thread->attr.wait = 1;
    readythds->remove_all(current_thread);

    schedule();
}

static void terminate_process(int signo)
{
    kill_current(signo);
}

static void terminate_process_with_core_dump(int signo)
{
    terminate_process(signo & 0x80);
}

static sig_t default_handlers[32] = {
    nullptr,
    terminate_process, // SIGHUP
    terminate_process, // SIGINT
    terminate_process_with_core_dump, // SIGQUIT
    terminate_process_with_core_dump, // SIGILL
    terminate_process_with_core_dump, // SIGTRAP
    terminate_process_with_core_dump, // SIGABRT, SIGIOT
    terminate_process_with_core_dump, // SIGBUS
    terminate_process_with_core_dump, // SIGFPE
    terminate_process, // SIGKILL
    terminate_process, // SIGUSR1
    terminate_process_with_core_dump, // SIGSEGV
    terminate_process, // SIGUSR2
    terminate_process, // SIGPIPE
    terminate_process, // SIGALRM
    terminate_process, // SIGTERM
    terminate_process, // SIGSTKFLT
    nullptr, // SIGCHLD
    continue_process, // SIGCONT
    stop_process, // SIGSTOP
    stop_process, // SIGTSTP
    stop_process, // SIGTTIN
    stop_process, // SIGTTOU
    nullptr, // SIGURG
    terminate_process_with_core_dump, // SIGXCPU
    terminate_process_with_core_dump, // SIGXFSZ
    terminate_process, // SIGVTALRM
    terminate_process, // SIGPROF
    nullptr, // SIGWINCH
    terminate_process, // SIGIO, SIGPOLL
    terminate_process, // SIGPWR
    terminate_process_with_core_dump, // SIGSYS, SIGUNUSED
};

signal_list::signal_list()
    : m_mask(0)
{
    memcpy(m_handlers, default_handlers, sizeof(m_handlers));
}

void signal_list::on_exec()
{
    for (int i = 1; i < 32; ++i) {
        if (m_handlers[i])
            m_handlers[i] = default_handlers[i];
    }
}

void signal_list::set(signo_type signal)
{
    if (m_mask & (1 << signal))
        return;

    if (!m_handlers[signal])
        return;

    m_list.push_back(signal);
    m_mask |= (1 << signal);
}

signo_type signal_list::handle()
{
    if (this->empty())
        return 0;

    auto signal = m_list.front();
    m_list.pop_front();

    if (!m_handlers[signal])
        return 0;

    m_handlers[signal](signal);

    return signal;
}

void signal_list::after_signal(signo_type signal)
{
    this->m_mask &= ~(1 << signal);
}

void signal_list::get_mask(sigset_t* mask) const
{
    if (!mask)
        return;

    memset(mask, 0x00, sizeof(sigset_t));
    for (int i = 1; i < 32; ++i) {
        if (is_masked(i))
            mask->__sig[(i-1)/4] |= 3 << (i-1) % 4 * 2;
    }
}

void signal_list::set_mask(const sigset_t* mask)
{
    if (!mask)
        return;

    m_mask = 0;
    for (int i = 1; i < 32; ++i) {
        if (mask->__sig[(i-1)/4] & (3 << (i-1) % 4 * 2))
            m_mask |= (1 << i);
    }
}
