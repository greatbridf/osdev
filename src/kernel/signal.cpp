#include <kernel/async/lock.hpp>
#include <kernel/interrupt.hpp>
#include <kernel/process.hpp>
#include <kernel/signal.hpp>
#include <kernel/task/thread.hpp>

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
    auto& parent = procs->find(current_process->ppid);

    // signal parent we're running
    parent.waitprocs.push_back({ current_process->pid, 0xffff });

    current_thread->signals.after_signal(signal);
}

static void stop_process(int signal)
{
    auto& parent = procs->find(current_process->ppid);

    current_thread->set_attr(kernel::task::thread::STOPPED);

    // signal parent we're stopped
    parent.waitprocs.push_back({ current_process->pid, 0x7f });
    parent.waitlist.notify_all();

    while (true) {
        if (schedule())
            break;
    }

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
    if (action.sa_handler == SIG_DFL)
        m_handlers.erase(signal);
    else
        m_handlers[signal] = action;
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

bool signal_list::raise(signo_type signal)
{
    async::lock_guard lck{m_mtx};

    // TODO: clear pending signals
    if (signal == SIGCONT) {
        m_list.remove_if([](signo_type sig) {
            return sig == SIGSTOP || sig == SIGTSTP
                || sig == SIGTTIN || sig == SIGTTOU;
        });
        return true;
    }

    if (sigmask(signal) & sigmask_stop) {
        m_list.remove(SIGCONT);
        return false;
    }

    auto iter = m_handlers.find(signal);
    if (iter != m_handlers.end()) {
        if (iter->second.sa_handler == SIG_IGN)
            return false;
    } else {
        if (sigmask(signal) & sigmask_ignore)
            return false;
    }

    m_list.push_back(signal);
    m_mask |= sigmask(signal);

    return true;
}

signo_type signal_list::pending_signal()
{
    async::lock_guard lck{m_mtx};
    for (auto iter = m_list.begin(); iter != m_list.end(); ++iter) {
        auto iter_handler = m_handlers.find(*iter);

        // signal default action
        if (iter_handler == m_handlers.end()) {
            if (!(sigmask(*iter) & sigmask_ignore))
                return *iter;
            iter = m_list.erase(iter);
            continue;
        }

        if (iter_handler->second.sa_handler == SIG_IGN) {
            iter = m_list.erase(iter);
            continue;
        }

        return *iter;
    }

    return 0;
}

void signal_list::handle(interrupt_stack_normal* context, mmx_registers* mmxregs)
{
    unsigned int signal;
    if (1) {
        async::lock_guard lck{m_mtx};
        // assume that the pending signal is at the front of the list
        signal = m_list.front();
        m_list.pop_front();
    }

    // default handlers
    if (sigmask(signal) & sigmask_now) {
        if (signal == SIGKILL)
            terminate_process(signal);
        else // SIGSTOP
            stop_process(signal);
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
        // signal is ignored by default
        return;
    }

    auto& handler = iter->second;
    if (!(handler.sa_flags & SA_RESTORER))
        raise(SIGSYS);

    // save current interrupt context to 128 bytes above current user stack
    uintptr_t sp = (uintptr_t)context->rsp;
    sp -= (128 + sizeof(mmx_registers) + sizeof(interrupt_stack_normal) + 16);
    sp &= ~0xf;

    auto tmpsp = sp;
    *(uint64_t*)tmpsp = signal; // signal handler argument: int signo
    tmpsp += 8;
    *(uintptr_t*)tmpsp = context->rsp; // original rsp
    tmpsp += 8;

    memcpy((void*)tmpsp, mmxregs, sizeof(mmx_registers));
    tmpsp += sizeof(mmx_registers); // mmx registers
    memcpy((void*)tmpsp, context, sizeof(interrupt_stack_normal));
    tmpsp += sizeof(interrupt_stack_normal); // context

    sp -= sizeof(void*);
    // signal handler return address: restorer
    *(uintptr_t*)sp = (uintptr_t)handler.sa_restorer;

    context->rsp = sp;
    context->v_rip = (uintptr_t)handler.sa_handler;
}

void signal_list::after_signal(signo_type signal)
{
    m_mask &= ~sigmask(signal);
}

kernel::sigmask_type signal_list::get_mask() const { return m_mask; }
void signal_list::set_mask(sigmask_type mask) { m_mask = mask & ~sigmask_now; }
void signal_list::mask(sigmask_type mask) { set_mask(m_mask | mask); }
void signal_list::unmask(sigmask_type mask) { set_mask(m_mask & ~mask); }
