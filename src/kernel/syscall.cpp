#include <asm/port_io.h>
#include <asm/sys.h>
#include <assert.h>
#include <bits/ioctl.h>
#include <errno.h>
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
#include <types/string.hpp>

#define SYSCALL_HANDLERS_SIZE (128)
syscall_handler syscall_handlers[SYSCALL_HANDLERS_SIZE];

extern "C" void _syscall_stub_fork_return(void);
int _syscall_fork(interrupt_stack* data)
{
    auto* newproc = &procs->emplace(*current_process)->value;
    auto* newthd = &newproc->thds.Emplace(*current_thread, newproc);
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

    return newproc->pid;
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

void __temporary_recursive_get_path(types::string<>& path, const fs::vfs::dentry* dent)
{
    if (dent == fs::fs_root)
        return;

    __temporary_recursive_get_path(path, dent->parent);
    path += '/';
    path += dent->name;
}

int _syscall_chdir(interrupt_stack* data)
{
    const char* path = reinterpret_cast<const char*>(data->s_regs.edi);
    auto* dir = fs::vfs_open_proc(path);
    if (!dir)
        return -ENOENT;

    if (!dir->ind->flags.in.directory)
        return -ENOTDIR;

    auto& pwd = current_process->pwd;
    pwd.clear();
    __temporary_recursive_get_path(pwd, dir);

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

    auto* dent = fs::vfs_open_proc(exec);
    if (!dent || !dent->ind)
        return -ENOENT;

    types::elf::elf32_load_data d;
    d.argv = argv;
    d.envp = envp;
    d.exec = dent->ind;
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
    if (current_thread->owner->thds.size() != 1) {
        assert(false);
    }

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

    return 0;
}

int _syscall_setsid(interrupt_stack*)
{
    if (current_process->pid == current_process->pgid)
        return -EPERM;

    current_process->sid = current_process->pid;
    current_process->pgid = current_process->pid;

    // TODO: get tty* from fd or block device id
    procs->set_ctrl_tty(current_process->pid, console);

    return current_process->pid;
}

int _syscall_getsid(interrupt_stack* data)
{
    pid_t pid = data->s_regs.edi;

    auto* proc = procs->find(pid);
    if (!proc)
        return -ESRCH;
    if (proc->sid != current_process->sid)
        return -EPERM;

    return proc->sid;
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

    auto* proc = procs->find(pid);
    // TODO: check whether the process exists
    // if (!proc)
    //     return -ESRCH;

    // TODO: check whether pgid and the original
    //       pgid is in the same session
    proc->pgid = pgid;

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
    if (!file)
        return -EBADF;

    if (file->type != fs::file::types::ind)
        return -ENOTTY;

    switch (request) {
    case TIOCGPGRP: {
        auto* pgid = (pid_t*)data->s_regs.edx;
        tty* ctrl_tty = procs->get_ctrl_tty(current_process->pid);
        // TODO: copy_to_user
        *pgid = ctrl_tty->get_pgrp();
        break;
    }
    case TIOCSPGRP: {
        // TODO: copy_from_user
        pid_t pgid = *(const pid_t*)data->s_regs.edx;
        tty* ctrl_tty = procs->get_ctrl_tty(current_process->pid);
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

int _syscall_brk(interrupt_stack* data)
{
    void* brk = (void*)data->s_regs.edi;

    if (brk < current_process->start_brk)
        return (int)current_process->brk;

    auto& mm = *current_process->mms.find(current_process->start_brk);

    // TODO: unmap released heap memory
    if (brk < current_process->brk)
        return (int)(current_process->brk = brk);

    ssize_t diff = align_up<12>((uint32_t)brk);
    diff -= align_up<12>((uint32_t)current_process->brk);
    diff /= 0x1000;
    for (ssize_t i = 0; i < diff; ++i)
        mm.append_page(empty_page, PAGE_COW, false);
    current_process->brk = brk;

    return (int)brk;
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
    syscall_handlers[12] = _syscall_brk;
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
