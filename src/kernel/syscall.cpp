#include <asm/port_io.h>
#include <asm/sys.h>
#include <kernel/errno.h>
#include <kernel/interrupt.h>
#include <kernel/log.hpp>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/syscall.hpp>
#include <kernel/vfs.hpp>
#include <kernel_main.hpp>
#include <stdint.h>
#include <stdio.h>
#include <types/allocator.hpp>
#include <types/assert.h>
#include <types/elf.hpp>
#include <types/status.h>

#define SYSCALL_SET_RETURN_VAL_EAX(_eax) \
    data->s_regs.eax = ((decltype(data->s_regs.eax))(_eax))

#define SYSCALL_SET_RETURN_VAL_EDX(_edx) \
    data->s_regs.edx = ((decltype(data->s_regs.edx))(_edx))

#define SYSCALL_SET_RETURN_VAL(_eax, _edx) \
    SYSCALL_SET_RETURN_VAL_EAX(_eax);      \
    SYSCALL_SET_RETURN_VAL_EDX(_edx)

#define SYSCALL_HANDLERS_SIZE (16)
syscall_handler syscall_handlers[SYSCALL_HANDLERS_SIZE];

void _syscall_not_impl(interrupt_stack* data)
{
    SYSCALL_SET_RETURN_VAL(0xffffffff, 0xffffffff);
}

extern "C" void _syscall_stub_fork_return(void);
void _syscall_fork(interrupt_stack* data)
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

    SYSCALL_SET_RETURN_VAL(newproc->pid, 0);
}

void _syscall_write(interrupt_stack* data)
{
    int fd = data->s_regs.edi;
    const char* buf = reinterpret_cast<const char*>(data->s_regs.esi);
    size_t n = data->s_regs.edx;

    auto* file = current_process->files[fd];
    if (file->type == fs::file::types::directory) {
        SYSCALL_SET_RETURN_VAL(GB_FAILED, EINVAL);
        return;
    }

    int n_wrote = fs::vfs_write(file->ind, buf, file->cursor, n);
    file->cursor += n_wrote;
    SYSCALL_SET_RETURN_VAL(n_wrote, 0);
}

void _syscall_read(interrupt_stack* data)
{
    int fd = data->s_regs.edi;
    char* buf = reinterpret_cast<char*>(data->s_regs.esi);
    size_t n = data->s_regs.edx;

    auto* file = current_process->files[fd];
    if (file->type == fs::file::types::directory) {
        SYSCALL_SET_RETURN_VAL(GB_FAILED, EINVAL);
        return;
    }

    // TODO: copy to user function !IMPORTANT
    int n_wrote = fs::vfs_read(file->ind, buf, n, file->cursor, n);
    file->cursor += n_wrote;
    SYSCALL_SET_RETURN_VAL(n_wrote, 0);
}

void _syscall_sleep(interrupt_stack* data)
{
    current_thread->attr.ready = 0;
    current_thread->attr.wait = 1;

    SYSCALL_SET_RETURN_VAL(0, 0);

    schedule();
}

void _syscall_crash(interrupt_stack*)
{
    kmsg("\nan error occurred while executing command\n");
    asm_cli();
    asm_hlt();
}

// syscall_exec(const char* exec, const char** argv)
// @param exec: the path of program to execute
// @param argv: arguments end with nullptr
void _syscall_exec(interrupt_stack* data)
{
    const char* exec = reinterpret_cast<const char*>(data->s_regs.edi);
    const char** argv = reinterpret_cast<const char**>(data->s_regs.esi);

    current_process->mms.clear_user();

    types::elf::elf32_load_data d;
    d.argv = argv;
    d.exec = exec;
    d.system = false;

    assert(types::elf::elf32_load(&d) == GB_OK);

    data->v_eip = d.eip;
    data->esp = (uint32_t)d.sp;
}

// @param exit_code
void _syscall_exit(interrupt_stack* data)
{
    uint32_t exit_code = data->s_regs.edi;
    pid_t pid = current_process->pid;
    pid_t ppid = current_process->ppid;

    // TODO: terminating a thread only
    if (current_thread->owner->thds.size() != 1) {
        _syscall_crash(data);
    }

    // terminating a whole process:

    // remove threads from ready list
    for (auto& thd : current_process->thds.underlying_list()) {
        thd.attr.ready = 0;
        readythds->remove_all(&thd);
    }

    // TODO: write back mmap'ped files and close them

    // unmap all user memory areas
    current_process->mms.clear_user();

    // make child processes orphans (children of init)
    procs->make_children_orphans(pid);

    current_process->attr.zombie = 1;

    // notify parent process and init
    auto* proc = procs->find(pid);
    auto* parent = procs->find(ppid);
    auto* init = procs->find(1);
    while (!proc->wait_lst.empty()) {
        init->wait_lst.push(proc->wait_lst.front());
    }
    parent->wait_lst.push({ current_thread, (void*)pid, (void*)exit_code, nullptr });

    // switch to new process and continue
    schedule();

    // we should not return to here
    assert(false);
}

// @param address of exit code: int*
// @return pid of the exited process
void _syscall_wait(interrupt_stack* data)
{
    auto* arg1 = reinterpret_cast<int*>(data->s_regs.edi);

    if (arg1 < (int*)0x40000000) {
        SYSCALL_SET_RETURN_VAL(-1, EINVAL);
        return;
    }

    auto& waitlst = current_process->wait_lst;
    if (waitlst.empty() && !procs->has_child(current_process->pid)) {
        SYSCALL_SET_RETURN_VAL(-1, ECHILD);
        return;
    }

    while (waitlst.empty()) {
        current_thread->attr.ready = 0;
        current_thread->attr.wait = 1;
        waitlst.subscribe(current_thread);

        schedule();

        if (!waitlst.empty()) {
            waitlst.unsubscribe(current_thread);
            break;
        }
    }

    auto evt = waitlst.front();

    pid_t pid = (pid_t)evt.data1;
    // TODO: copy_to_user check privilege
    *arg1 = (int)evt.data2;

    procs->remove(pid);
    SYSCALL_SET_RETURN_VAL(pid, 0);
}

void _syscall_getdents(interrupt_stack* data)
{
    int fd = data->s_regs.edi;
    auto* buf = (char*)(data->s_regs.esi);
    size_t cnt = data->s_regs.edx;

    auto* dir = current_process->files[fd];
    if (dir->type != fs::file::types::directory) {
        data->s_regs.eax = -1;
        return;
    }

    size_t orig_cnt = cnt;
    int nread = dir->ind->fs->inode_readdir(dir->ind, dir->cursor,
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

    data->s_regs.eax = orig_cnt - cnt;
}

void _syscall_open(interrupt_stack* data)
{
    auto* path = (const char*)data->s_regs.edi;
    // flags are ignored for now
    uint32_t flags = data->s_regs.esi;
    data->s_regs.eax = current_process->files.open(path, flags);
}

void init_syscall(void)
{
    syscall_handlers[0] = _syscall_fork;
    syscall_handlers[1] = _syscall_write;
    syscall_handlers[2] = _syscall_sleep;
    syscall_handlers[3] = _syscall_crash;
    syscall_handlers[4] = _syscall_exec;
    syscall_handlers[5] = _syscall_exit;
    syscall_handlers[6] = _syscall_wait;
    syscall_handlers[7] = _syscall_read;
    syscall_handlers[8] = _syscall_getdents;
    syscall_handlers[9] = _syscall_open;
}
