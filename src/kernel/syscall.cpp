#include <asm/port_io.h>
#include <asm/sys.h>
#include <assert.h>
#include <bits/ioctl.h>
#include <kernel/errno.h>
#include <kernel/interrupt.h>
#include <kernel/log.hpp>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/syscall.hpp>
#include <kernel/tty.hpp>
#include <kernel/vfs.hpp>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <types/allocator.hpp>
#include <types/elf.hpp>
#include <types/lock.hpp>
#include <types/status.h>

#define SYSCALL_HANDLERS_SIZE (128)
syscall_handler syscall_handlers[SYSCALL_HANDLERS_SIZE];

extern "C" void _syscall_stub_fork_return(void);
int _syscall_fork(interrupt_stack* data)
{
    auto& newproc = procs->copy_from(*current_process);
    auto [ iter_newthd, inserted ] = newproc.thds.emplace(*current_thread, newproc.pid);
    assert(inserted);
    auto* newthd = &*iter_newthd;

    readythds->push(newthd);

    // create fake interrupt stack
    push_stack(&newthd->esp, data->ss);
    push_stack(&newthd->esp, data->esp);
    push_stack(&newthd->esp, data->eflags);
    push_stack(&newthd->esp, data->cs);
    push_stack(&newthd->esp, (uint32_t)data->v_eip);

    // eax
    push_stack(&newthd->esp, 0);
    push_stack(&newthd->esp, data->s_regs.ecx);
    // edx
    push_stack(&newthd->esp, 0);
    push_stack(&newthd->esp, data->s_regs.ebx);
    push_stack(&newthd->esp, data->s_regs.esp);
    push_stack(&newthd->esp, data->s_regs.ebp);
    push_stack(&newthd->esp, data->s_regs.esi);
    push_stack(&newthd->esp, data->s_regs.edi);

    // ctx_switch stack
    // return address
    push_stack(&newthd->esp, (uint32_t)_syscall_stub_fork_return);
    // ebx
    push_stack(&newthd->esp, 0);
    // edi
    push_stack(&newthd->esp, 0);
    // esi
    push_stack(&newthd->esp, 0);
    // ebp
    push_stack(&newthd->esp, 0);
    // eflags
    push_stack(&newthd->esp, 0);

    return newproc.pid;
}

int _syscall_write(interrupt_stack* data)
{
    int fd = data->s_regs.edi;
    const char* buf = reinterpret_cast<const char*>(data->s_regs.esi);
    size_t n = data->s_regs.edx;

    auto* file = current_process->files[fd];

    if (!file || !file->flags.write)
        return -EBADF;

    switch (file->type) {
    case fs::file::types::ind: {
        if (file->ptr.ind->flags.in.directory)
            return -EBADF;

        int n_wrote = fs::vfs_write(file->ptr.ind, buf, file->cursor, n);
        if (n_wrote >= 0)
            file->cursor += n_wrote;
        return n_wrote;
    }
    case fs::file::types::pipe:
        return file->ptr.pp->write(buf, n);

    case fs::file::types::socket:
        // TODO
        return -EINVAL;
    default:
        assert(false);
        for ( ; ; ) ;
    }
}

int _syscall_read(interrupt_stack* data)
{
    int fd = data->s_regs.edi;
    char* buf = reinterpret_cast<char*>(data->s_regs.esi);
    size_t n = data->s_regs.edx;

    auto* file = current_process->files[fd];

    if (!file || !file->flags.read)
        return -EBADF;

    switch (file->type) {
    case fs::file::types::ind: {
        if (file->ptr.ind->flags.in.directory)
            return -EBADF;

        // TODO: copy to user function !IMPORTANT
        int n_wrote = fs::vfs_read(file->ptr.ind, buf, n, file->cursor, n);
        if (n_wrote >= 0)
            file->cursor += n_wrote;
        return n_wrote;
    }
    case fs::file::types::pipe:
        return file->ptr.pp->read(buf, n);

    case fs::file::types::socket:
        // TODO
        return -EINVAL;
    default:
        assert(false);
        for ( ; ; ) ;
    }
}

// TODO: sleep seconds
int _syscall_sleep(interrupt_stack*)
{
    current_thread->attr.ready = 0;
    current_thread->attr.wait = 1;

    schedule();
    return 0;
}

int _syscall_chdir(interrupt_stack* data)
{
    const char* path = reinterpret_cast<const char*>(data->s_regs.edi);
    auto* dir = fs::vfs_open(path);
    if (!dir)
        return -ENOENT;

    if (!dir->ind->flags.in.directory)
        return -ENOTDIR;

    current_process->pwd = path;

    return 0;
}

// syscall_exec(const char* exec, const char** argv)
// @param exec: the path of program to execute
// @param argv: arguments end with nullptr
// @param envp: environment variables end with nullptr
int _syscall_execve(interrupt_stack* data)
{
    const char* exec = reinterpret_cast<const char*>(data->s_regs.edi);
    char* const* argv = reinterpret_cast<char* const*>(data->s_regs.esi);
    char* const* envp = reinterpret_cast<char* const*>(data->s_regs.edx);

    types::elf::elf32_load_data d;
    d.argv = argv;
    d.envp = envp;
    d.exec = exec;
    d.system = false;

    int ret = types::elf::elf32_load(&d);
    if (ret != GB_OK)
        return -d.errcode;

    data->v_eip = d.eip;
    data->esp = (uint32_t)d.sp;

    return 0;
}

// @param exit_code
int NORETURN _syscall_exit(interrupt_stack* data)
{
    uint32_t exit_code = data->s_regs.edi;

    // TODO: terminating a thread only
    if (current_process->thds.size() != 1)
        assert(false);

    // terminating a whole process:
    procs->kill(current_process->pid, exit_code);

    // switch to new process and continue
    schedule_noreturn();
}

// @param address of exit code: int*
// @return pid of the exited process
int _syscall_wait(interrupt_stack* data)
{
    auto* arg1 = reinterpret_cast<int*>(data->s_regs.edi);

    auto& cv = current_process->cv_wait;
    auto& mtx = cv.mtx();
    types::lock_guard lck(mtx);

    auto& waitlist = current_process->waitlist;

    while (waitlist.empty()) {
        if (!procs->has_child(current_process->pid))
            return -ECHILD;

        if (!cv.wait(mtx))
            return -EINTR;
    }

    auto iter = waitlist.begin();
    assert(iter != waitlist.end());

    auto& obj = *iter;
    pid_t pid = obj.pid;

    // TODO: copy_to_user check privilege
    *arg1 = obj.code;

    procs->remove(pid);
    waitlist.erase(iter);

    return pid;
}

int _syscall_getdents(interrupt_stack* data)
{
    int fd = data->s_regs.edi;
    auto* buf = (char*)(data->s_regs.esi);
    size_t cnt = data->s_regs.edx;

    auto* dir = current_process->files[fd];
    if (dir->type != fs::file::types::ind || !dir->ptr.ind->flags.in.directory)
        return -ENOTDIR;

    size_t orig_cnt = cnt;
    int nread = dir->ptr.ind->fs->inode_readdir(dir->ptr.ind, dir->cursor,
        [&buf, &cnt](const char* fn, size_t len, fs::ino_t ino, uint8_t type) -> int {
            if (!len)
                len = strlen(fn);

            size_t reclen = sizeof(fs::user_dirent) + 1 + len;
            if (cnt < reclen)
                return GB_FAILED;

            auto* dirp = (fs::user_dirent*)buf;
            dirp->d_ino = ino;
            dirp->d_reclen = reclen;
            // TODO: show offset
            // dirp->d_off = 0;
            // TODO: use copy_to_user
            memcpy(dirp->d_name, fn, len);
            buf[reclen - 2] = 0;
            buf[reclen - 1] = type;

            buf += reclen;
            cnt -= reclen;
            return GB_OK;
        });

    if (nread > 0)
        dir->cursor += nread;

    return orig_cnt - cnt;
}

int _syscall_open(interrupt_stack* data)
{
    auto* path = (const char*)data->s_regs.edi;
    uint32_t flags = data->s_regs.esi;
    return current_process->files.open(path, flags);
}

int _syscall_getcwd(interrupt_stack* data)
{
    char* buf = reinterpret_cast<char*>(data->s_regs.edi);
    size_t bufsize = reinterpret_cast<size_t>(data->s_regs.esi);

    // TODO: use copy_to_user
    strncpy(buf, current_process->pwd.c_str(), bufsize);
    buf[bufsize - 1] = 0;

    return (uint32_t)buf;
}

int _syscall_setsid(interrupt_stack*)
{
    if (current_process->pid == current_process->pgid)
        return -EPERM;

    current_process->sid = current_process->pid;
    current_process->pgid = current_process->pid;

    // TODO: get tty* from fd or block device id
    console->set_pgrp(current_process->pid);
    current_process->control_tty = console;

    return current_process->pid;
}

int _syscall_getsid(interrupt_stack* data)
{
    pid_t pid = data->s_regs.edi;

    if (!procs->try_find(pid))
        return -ESRCH;
    auto& proc = procs->find(pid);
    if (proc.sid != current_process->sid)
        return -EPERM;

    return proc.sid;
}

int _syscall_close(interrupt_stack* data)
{
    int fd = data->s_regs.edi;
    current_process->files.close(fd);
    return 0;
}

int _syscall_dup(interrupt_stack* data)
{
    int old_fd = data->s_regs.edi;
    return current_process->files.dup(old_fd);
}

int _syscall_dup2(interrupt_stack* data)
{
    int old_fd = data->s_regs.edi;
    int new_fd = data->s_regs.esi;
    return current_process->files.dup2(old_fd, new_fd);
}

int _syscall_pipe(interrupt_stack* data)
{
    auto& pipefd = *(int(*)[2])data->s_regs.edi;
    return current_process->files.pipe(pipefd);
}

int _syscall_setpgid(interrupt_stack* data)
{
    pid_t pid = data->s_regs.edi;
    pid_t pgid = data->s_regs.esi;

    if (pgid < 0)
        return -EINVAL;

    if (pid == 0)
        pid = current_process->pid;

    if (pgid == 0)
        pgid = pid;

    if (!procs->try_find(pid))
        return -ESRCH;

    auto& proc = procs->find(pid);

    // TODO: check whether pgid and the original
    //       pgid is in the same session

    proc.pgid = pgid;

    return 0;
}

int _syscall_ioctl(interrupt_stack* data)
{
    int fd = data->s_regs.edi;
    unsigned long request = data->s_regs.esi;

    // TODO: check fd type and get tty* from fd
    //
    //       we use a trick for now, check whether
    //       the file that fd points to is a pipe or
    //       not. and we suppose that stdin will be
    //       either a tty or a pipe.
    auto* file = current_process->files[fd];
    if (!file || file->type != fs::file::types::ind)
        return -ENOTTY;

    switch (request) {
    case TIOCGPGRP: {
        auto* pgid = (pid_t*)data->s_regs.edx;
        tty* ctrl_tty = current_process->control_tty;
        // TODO: copy_to_user
        *pgid = ctrl_tty->get_pgrp();
        break;
    }
    case TIOCSPGRP: {
        // TODO: copy_from_user
        pid_t pgid = *(const pid_t*)data->s_regs.edx;
        tty* ctrl_tty = current_process->control_tty;
        ctrl_tty->set_pgrp(pgid);
        break;
    }
    default:
        return -EINVAL;
    }

    return 0;
}

int _syscall_getpid(interrupt_stack*)
{
    return current_process->pid;
}

int _syscall_getppid(interrupt_stack*)
{
    return current_process->ppid;
}

extern "C" void syscall_entry(interrupt_stack* data)
{
    int syscall_no = data->s_regs.eax;
    if (syscall_no >= SYSCALL_HANDLERS_SIZE)
        kill_current(-1);

    int ret = syscall_handlers[syscall_no](data);

    data->s_regs.eax = ret;

    check_signal();
}

SECTION(".text.kinit")
void init_syscall(void)
{
    memset(syscall_handlers, 0x00, sizeof(syscall_handlers));

    syscall_handlers[0] = _syscall_read;
    syscall_handlers[1] = _syscall_write;
    syscall_handlers[2] = _syscall_open;
    syscall_handlers[3] = _syscall_close;
    syscall_handlers[16] = _syscall_ioctl;
    syscall_handlers[22] = _syscall_pipe;
    syscall_handlers[32] = _syscall_dup;
    syscall_handlers[33] = _syscall_dup2;
    syscall_handlers[35] = _syscall_sleep;
    syscall_handlers[39] = _syscall_getpid;
    syscall_handlers[57] = _syscall_fork;
    syscall_handlers[59] = _syscall_execve;
    syscall_handlers[60] = _syscall_exit;
    syscall_handlers[61] = _syscall_wait;
    syscall_handlers[78] = _syscall_getdents;
    syscall_handlers[79] = _syscall_getcwd;
    syscall_handlers[80] = _syscall_chdir;
    syscall_handlers[109] = _syscall_setpgid;
    syscall_handlers[110] = _syscall_getppid;
    syscall_handlers[112] = _syscall_setsid;
    syscall_handlers[124] = _syscall_getsid;
}
