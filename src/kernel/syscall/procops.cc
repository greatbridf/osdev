#include <sys/prctl.h>
#include <sys/utsname.h>
#include <sys/wait.h>

#include <types/elf.hpp>

#include <kernel/log.hpp>
#include <kernel/process.hpp>
#include <kernel/signal.hpp>
#include <kernel/syscall.hpp>
#include <kernel/utsname.hpp>

using namespace kernel::syscall;

#define NOT_IMPLEMENTED not_implemented(__FILE__, __LINE__)

static inline void not_implemented(const char* pos, int line)
{
    kmsgf("[kernel] the function at %s:%d is not implemented, killing the pid%d...",
            pos, line, current_process->pid);
    current_thread->send_signal(SIGSYS);
}

int kernel::syscall::do_chdir(const char __user* path)
{
    auto* dir = fs::vfs_open(*current_process->root,
            current_process->pwd + path);
    if (!dir)
        return -ENOENT;

    if (!S_ISDIR(dir->ind->mode))
        return -ENOTDIR;

    current_process->pwd.clear();
    dir->path(*current_process->root, current_process->pwd);

    return 0;
}

execve_retval kernel::syscall::do_execve(
        const char __user* exec,
        char __user* const __user* argv,
        char __user* const __user* envp)
{
    types::elf::elf32_load_data d;

    if (!exec || !argv || !envp)
        return { 0, 0, -EFAULT };

    // TODO: use copy_from_user
    while (*argv)
        d.argv.push_back(*(argv++));

    while (*envp)
        d.envp.push_back(*(envp++));

    d.exec_dent = fs::vfs_open(*current_process->root,
            current_process->pwd + exec);

    if (!d.exec_dent)
        return { 0, 0, -ENOENT };

    current_process->files.onexec();

    // TODO: set cs and ss to compatibility mode
    if (int ret = types::elf::elf32_load(d); ret != 0)
        return { 0, 0, ret };

    current_thread->signals.on_exec();

    return { d.ip, d.sp, 0 };
}


int kernel::syscall::do_exit(int status)
{
    // TODO: terminating a thread only
    assert(current_process->thds.size() == 1);

    // terminating a whole process:
    procs->kill(current_process->pid, (status & 0xff) << 8);

    // switch to new process and continue
    schedule_noreturn();
}

int kernel::syscall::do_waitpid(pid_t waitpid, int __user* arg1, int options)
{
    if (waitpid != -1)
        return -EINVAL;

    auto& cv = current_process->waitlist;
    kernel::async::lock_guard lck(current_process->mtx_waitprocs);

    auto& waitlist = current_process->waitprocs;

    // TODO: check if it is waiting for stopped process
    if (options & ~(WNOHANG | WUNTRACED)) {
        NOT_IMPLEMENTED;
        return -EINVAL;
    }

    while (waitlist.empty()) {
        if (current_process->children.empty())
            return -ECHILD;

        if (options & WNOHANG)
            return 0;

        bool interrupted = cv.wait(current_process->mtx_waitprocs);
        if (interrupted)
            return -EINTR;
    }

    for (auto iter = waitlist.begin(); iter != waitlist.end(); ++iter) {
        if (WIFSTOPPED(iter->code) && !(options & WUNTRACED))
            continue;

        pid_t pid = iter->pid;

        // TODO: copy_to_user
        *arg1 = iter->code;

        procs->remove(pid);
        waitlist.erase(iter);

        return pid;
    }

    // we should never reach here
    freeze();
    return -EINVAL;
}

char __user* kernel::syscall::do_getcwd(char __user* buf, size_t buf_size)
{
    // TODO: use copy_to_user
    auto path = current_process->pwd.full_path();
    strncpy(buf, path.c_str(), buf_size);
    buf[buf_size - 1] = 0;

    return buf;
}

pid_t kernel::syscall::do_setsid()
{
    if (current_process->pid == current_process->pgid)
        return -EPERM;

    current_process->sid = current_process->pid;
    current_process->pgid = current_process->pid;

    // TODO: get tty* from fd or block device id
    tty::console->set_pgrp(current_process->pid);
    current_process->control_tty = tty::console;

    return current_process->pid;
}

pid_t kernel::syscall::do_getsid(pid_t pid)
{
    auto [ pproc, found ] = procs->try_find(pid);
    if (!found)
        return -ESRCH;
    if (pproc->sid != current_process->sid)
        return -EPERM;

    return pproc->sid;
}

int kernel::syscall::do_setpgid(pid_t pid, pid_t pgid)
{
    if (pgid < 0)
        return -EINVAL;

    if (pid == 0)
        pid = current_process->pid;

    if (pgid == 0)
        pgid = pid;

    auto [ pproc, found ] = procs->try_find(pid);
    if (!found)
        return -ESRCH;

    // TODO: check whether pgid and the original
    //       pgid is in the same session

    pproc->pgid = pgid;

    return 0;
}

int kernel::syscall::do_set_thread_area(kernel::user::user_desc __user* ptr)
{
    auto ret = current_thread->set_thread_area(ptr);
    if (ret != 0)
        return ret;

    current_thread->load_thread_area32();
    return 0;
}

pid_t kernel::syscall::do_set_tid_address(int __user* tidptr)
{
    // TODO: copy_from_user
    current_thread->set_child_tid = tidptr;
    return current_thread->tid();
}

int kernel::syscall::do_prctl(int option, uintptr_t arg2)
{
    switch (option) {
    case PR_SET_NAME: {
        // TODO: copy_from_user
        auto* name = (const char __user*)arg2;
        current_thread->name.assign(name, 15);
        break;
    }
    case PR_GET_NAME: {
        auto* name = (char __user*)arg2;
        // TODO: copy_to_user
        strncpy(name, current_thread->name.c_str(), 16);
        name[15] = 0;
        break;
    }
    default:
        return -EINVAL;
    }

    return 0;
}

int kernel::syscall::do_arch_prctl(int option, uintptr_t arg2)
{
    switch (option) {
    case PR_SET_NAME: {
        // TODO: copy_from_user
        auto* name = (const char __user*)arg2;
        current_thread->name.assign(name, 15);
        break;
    }
    case PR_GET_NAME: {
        auto* name = (char __user*)arg2;
        // TODO: copy_to_user
        strncpy(name, current_thread->name.c_str(), 16);
        name[15] = 0;
        break;
    }
    default:
        return -EINVAL;
    }

    return 0;
}

int kernel::syscall::do_umask(mode_t mask)
{
    mode_t old = current_process->umask;
    current_process->umask = mask;

    return old;
}

int kernel::syscall::do_kill(pid_t pid, int sig)
{
    auto [ pproc, found ] = procs->try_find(pid);
    if (!found)
        return -ESRCH;

    if (!kernel::signal_list::check_valid(sig))
        return -EINVAL;

    if (pproc->is_system())
        return 0;

    // TODO: check permission
    procs->send_signal(pid, sig);

    return 0;
}

int kernel::syscall::do_rt_sigprocmask(int how, const sigmask_type __user* set,
        sigmask_type __user* oldset, size_t sigsetsize)
{
    if (sigsetsize != sizeof(sigmask_type))
        return -EINVAL;

    sigmask_type sigs = current_thread->signals.get_mask();

    // TODO: use copy_to_user
    if (oldset)
        memcpy(oldset, &sigs, sizeof(sigmask_type));

    if (!set)
        return 0;

    // TODO: use copy_from_user
    switch (how) {
    case SIG_BLOCK:
        current_thread->signals.mask(*set);
        break;
    case SIG_UNBLOCK:
        current_thread->signals.unmask(*set);
        break;
    case SIG_SETMASK:
        current_thread->signals.set_mask(*set);
        break;
    }

    return 0;
}

int kernel::syscall::do_rt_sigaction(int signum, const sigaction __user* act,
        sigaction __user* oldact, size_t sigsetsize)
{
    if (sigsetsize != sizeof(sigmask_type))
        return -EINVAL;

    if (!kernel::signal_list::check_valid(signum)
        || signum == SIGKILL || signum == SIGSTOP)
        return -EINVAL;

    // TODO: use copy_to_user
    if (oldact)
        current_thread->signals.get_handler(signum, *oldact);

    if (!act)
        return 0;

    // TODO: use copy_from_user
    current_thread->signals.set_handler(signum, *act);

    return 0;
}

int kernel::syscall::do_newuname(new_utsname __user* buf)
{
    if (!buf)
        return -EFAULT;

    // TODO: use copy_to_user
    memcpy(buf, sys_utsname, sizeof(new_utsname));

    return 0;
}

pid_t kernel::syscall::do_getpgid(pid_t pid)
{
    if (pid == 0)
        return current_process->pgid;

    auto [ pproc, found ] = procs->try_find(pid);
    if (!found)
        return -ESRCH;

    return pproc->pgid;
}

pid_t kernel::syscall::do_getpid()
{
    return current_process->pid;
}

pid_t kernel::syscall::do_getppid()
{
    return current_process->ppid;
}

uid_t kernel::syscall::do_getuid()
{
    return 0; // all users are root for now
}

uid_t kernel::syscall::do_geteuid()
{
    return 0; // all users are root for now
}

gid_t kernel::syscall::do_getgid()
{
    return 0; // all users are root for now
}

pid_t kernel::syscall::do_gettid()
{
    return current_thread->tid();
}

uintptr_t kernel::syscall::do_brk(uintptr_t addr)
{
    return current_process->mms.set_brk(addr);
}
