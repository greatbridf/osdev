#include <kernel/process.hpp>
#include <kernel/signal.hpp>

#include <signal.h>

#define sigmask(sig) (1ULL << ((sig)-1))

#define sigmask_now (sigmask(SIGKILL) | sigmask(SIGSTOP))

#define sigmask_ignore (sigmask(SIGCHLD) | sigmask(SIGURG) | sigmask(SIGWINCH))

#define sigmask_coredump (\
    sigmask(SIGQUIT) | sigmask(SIGILL) | sigmask(SIGTRAP) | sigmask(SIGABRT) | \
    sigmask(SIGFPE) | sigmask(SIGSEGV) | sigmask(SIGBUS) | sigmask(SIGSYS) | \
    sigmask(SIGXCPU) | sigmask(SIGXFSZ) )

#define sigmask_stop (\
    sigmask(SIGSTOP) | sigmask(SIGTSTP) | sigmask(SIGTTIN) | sigmask(SIGTTOU))

using kernel::signal_list;
using signo_type = signal_list::signo_type;

static void continue_process(int signal)
{
    current_thread->signals.after_signal(signal);
}

static void stop_process(int signal)
{
    current_thread->sleep();

    schedule();

    current_thread->signals.after_signal(signal);
}

static void terminate_process(int signo)
{
    kill_current(signo);
}

static void terminate_process_with_core_dump(int signo)
{
    terminate_process(signo & 0x80);
}

void signal_list::set_handler(signo_type signal, const sigaction& action)
{
    if (action.sa_handler == SIG_DFL) {
        m_handlers.erase(signal);
        return;
    }
    else {
        m_handlers[signal] = action;
    }
}

void signal_list::get_handler(signo_type signal, sigaction& action) const
{
    auto iter = m_handlers.find(signal);
    if (iter == m_handlers.end()) {
        action.sa_handler = SIG_DFL;
        action.sa_flags = 0;
        action.sa_restorer = nullptr;
        action.sa_mask = 0;
    }
    else {
        action = iter->second;
    }
}

void signal_list::on_exec()
{
    std::erase_if(m_handlers, [](auto& pair) {
        return pair.second.sa_handler != SIG_IGN;
    });
}

void signal_list::raise(signo_type signal)
{
    // TODO: clear pending signals
    // if (signal == SIGCONT) {
    //     m_list.remove_if([](signo_type sig) {
    //         return sig == SIGSTOP || sig == SIGTSTP
    //             || sig == SIGTTIN || sig == SIGTTOU;
    //     });
    // }

    // if (signal == SIGSTOP)
    //     m_list.remove(SIGCONT);

    if (m_mask & sigmask(signal))
        return;

    auto iter = m_handlers.find(signal);
    if (iter != m_handlers.end()) {
        if (iter->second.sa_handler == SIG_IGN)
            return;
    }

    m_list.push_back(signal);
    m_mask |= sigmask(signal);
}

signo_type signal_list::handle()
{
    if (this->empty())
        return 0;

    auto signal = m_list.front();
    m_list.pop_front();

    // default handlers
    if (sigmask(signal) & sigmask_now) {
        if (signal == SIGKILL)
            terminate_process(signal);
        else // SIGSTOP
            stop_process(signal);
        return signal;
    }

    auto iter = m_handlers.find(signal);
    if (iter == m_handlers.end()) {
        if (signal == SIGCONT)
            continue_process(signal);
        else if (sigmask(signal) & sigmask_stop)
            stop_process(signal);
        else if (sigmask(signal) & sigmask_coredump)
            terminate_process_with_core_dump(signal);
        else if (!(sigmask(signal) & sigmask_ignore))
            terminate_process(signal);
        else // signal is ignored by default
            return 0;
    }
    else {
        iter->second.sa_handler(signal);
    }

    return signal;
}

void signal_list::after_signal(signo_type signal)
{
    this->m_mask &= ~sigmask(signal);
}

kernel::sigmask_type signal_list::get_mask() const { return m_mask; }
void signal_list::set_mask(sigmask_type mask) { m_mask = mask & ~sigmask_now; }
void signal_list::mask(sigmask_type mask) { set_mask(m_mask | mask); }
void signal_list::unmask(sigmask_type mask) { set_mask(m_mask & ~mask); }
