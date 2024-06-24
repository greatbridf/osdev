#include <memory>
#include <queue>
#include <utility>

#include <assert.h>
#include <bits/alltypes.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/mount.h>
#include <sys/wait.h>

#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/elf.hpp>
#include <types/types.h>

#include <kernel/async/lock.hpp>
#include <kernel/log.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/module.hpp>
#include <kernel/process.hpp>
#include <kernel/signal.hpp>
#include <kernel/task/readyqueue.hpp>
#include <kernel/task/thread.hpp>
#include <kernel/user/thread_local.hpp>
#include <kernel/vfs.hpp>

using kernel::async::mutex;
using kernel::async::lock_guard, kernel::async::lock_guard_irq;

static void (*volatile kthreadd_new_thd_func)(void*);
static void* volatile kthreadd_new_thd_data;

static mutex kthreadd_mtx;

namespace kernel {

struct no_irq_guard {
    explicit no_irq_guard()
    {
        asm volatile("cli");
    }

    no_irq_guard(const no_irq_guard&) = delete;
    no_irq_guard& operator=(const no_irq_guard&) = delete;

    ~no_irq_guard()
    {
        asm volatile("sti");
    }
};

} // namespace kernel

int filearr::allocate_fd(int from)
{
    if (from < min_avail)
        from = min_avail;

    if (from == min_avail) {
        int nextfd = min_avail + 1;
        auto iter = arr.find(nextfd);
        while (iter != arr.end() && nextfd == iter->first)
            ++nextfd, ++iter;

        int retval = min_avail;
        min_avail = nextfd;
        return retval;
    }

    int fd = from;
    auto iter = arr.find(fd);
    while (iter != arr.end() && fd == iter->first)
        ++fd, ++iter;

    return fd;
}

void filearr::release_fd(int fd)
{
    if (fd < min_avail)
        min_avail = fd;
}

int filearr::dup(int old_fd)
{
    return dup2(old_fd, next_fd());
}

int filearr::dup2(int old_fd, int new_fd)
{
    close(new_fd);

    auto iter = arr.find(old_fd);
    if (!iter)
        return -EBADF;

    int fd = allocate_fd(new_fd);
    assert(fd == new_fd);

    auto [ newiter, inserted ] = this->arr.emplace(new_fd, iter->second);
    assert(inserted);

    newiter->second.flags = 0;

    return new_fd;
}

int filearr::dupfd(int fd, int minfd, int flags)
{
    auto iter = arr.find(fd);
    if (!iter)
        return -EBADF;

    int new_fd = allocate_fd(minfd);
    auto [ newiter, inserted ] = arr.emplace(new_fd, iter->second);
    assert(inserted);

    newiter->second.flags = flags;
    return new_fd;
}

int filearr::set_flags(int fd, int flags)
{
    auto iter = arr.find(fd);
    if (!iter)
        return -EBADF;
    iter->second.flags |= flags;
    return 0;
}

int filearr::clear_flags(int fd, int flags)
{
    auto iter = arr.find(fd);
    if (!iter)
        return -EBADF;
    iter->second.flags &= ~flags;
    return 0;
}

// TODO: file opening permissions check
int filearr::open(const process &current,
    const types::path& filepath, int flags, mode_t mode)
{
    auto* dentry = fs::vfs_open(*current.root, filepath);

    if (flags & O_CREAT) {
        if (!dentry) {
            // create file
            auto filename = filepath.last_name();
            auto parent_path = filepath;
            parent_path.remove_last();

            auto* parent = fs::vfs_open(*current.root, parent_path);
            if (!parent)
                return -EINVAL;
            int ret = fs::vfs_mkfile(parent, filename.c_str(), mode);
            if (ret != 0)
                return ret;
            dentry = fs::vfs_open(*current.root, filepath);
            assert(dentry);
        } else {
            // file already exists
            if (flags & O_EXCL)
                return -EEXIST;
        }
    } else {
        if (!dentry)
            return -ENOENT;
    }

    auto filemode = dentry->ind->mode;

    // check whether dentry is a file if O_DIRECTORY is set
    if (flags & O_DIRECTORY) {
        if (!S_ISDIR(filemode))
            return -ENOTDIR;
    } else {
        if (S_ISDIR(filemode) && (flags & (O_WRONLY | O_RDWR)))
            return -EISDIR;
    }

    // truncate file
    if (flags & O_TRUNC) {
        if ((flags & (O_WRONLY | O_RDWR)) && S_ISREG(filemode)) {
            auto ret = fs::vfs_truncate(dentry->ind, 0);
            if (ret != 0)
                return ret;
        }
    }

    int fdflag = (flags & O_CLOEXEC) ? FD_CLOEXEC : 0;

    int fd = next_fd();
    auto [ _, inserted ] = arr.emplace(fd, fditem {
        fdflag, std::shared_ptr<fs::file> {
            new fs::regular_file(dentry->parent, {
                .read = !(flags & O_WRONLY),
                .write = !!(flags & (O_WRONLY | O_RDWR)),
                .append = !!(S_ISREG(filemode) && flags & O_APPEND),
            }, 0, dentry->ind),
    } } );
    assert(inserted);
    return fd;
}

process::process(const process& parent, pid_t pid)
    : mms { parent.mms }, attr { parent.attr } , files { parent.files }
    , pwd { parent.pwd }, umask { parent.umask }, pid { pid }
    , ppid { parent.pid }, pgid { parent.pgid } , sid { parent.sid }
    , control_tty { parent.control_tty }, root { parent.root } { }

process::process(pid_t pid, pid_t ppid)
    : attr { .system = true }
    , pwd { "/" } , pid { pid } , ppid { ppid }
{
    bool inserted;
    std::tie(std::ignore, inserted) = thds.emplace("", pid);
    assert(inserted);
}

using signo_type = kernel::signal_list::signo_type;

void process::send_signal(signo_type signal)
{
    for (auto& thd : thds)
        thd.send_signal(signal);
}

void kernel_threadd_main(void)
{
    kmsg("kernel thread daemon started");

    for (;;) {
        if (kthreadd_new_thd_func) {
            void (*func)(void*) = nullptr;
            void* data = nullptr;

            if (1) {
                lock_guard lck(kthreadd_mtx);

                if (kthreadd_new_thd_func) {
                    func = std::exchange(kthreadd_new_thd_func, nullptr);
                    data = std::exchange(kthreadd_new_thd_data, nullptr);
                }
            }

            // TODO
            (void)func, (void)data;
            assert(false);

            // syscall_fork
            // int ret = syscall(0x00);

            // if (ret == 0) {
            //     // child process
            //     func(data);
            //     // the function shouldn't return here
            //     assert(false);
            // }
        }
        // TODO: sleep here to wait for new_kernel_thread event
        asm volatile("hlt");
    }
}

SECTION(".text.kinit")
proclist::proclist()
{
    // init process has no parent
    auto& init = real_emplace(1, 0);
    assert(init.pid == 1 && init.ppid == 0);

    auto& thd = *init.thds.begin();
    thd.name.assign("[kernel init]");

    current_process = &init;
    current_thread = &thd;

    kernel::task::dispatcher::enqueue(current_thread);

    // TODO: LONG MODE
    // tss.ss0 = KERNEL_DATA_SEGMENT;
    // tss.esp0 = (uint32_t)current_thread->kstack.esp;

    current_process->mms.switch_pd();

    if (1) {
        // pid 0 is kernel thread daemon
        auto& proc = real_emplace(0, 0);
        assert(proc.pid == 0 && proc.ppid == 0);

        // create thread
        auto& thd = *proc.thds.begin();
        thd.name.assign("[kernel thread daemon]");

        // TODO: LONG MODE
        // auto* esp = &thd.kstack.esp;
        // auto old_esp = (uint32_t)thd.kstack.esp;

        // // return(start) address
        // push_stack(esp, (uint32_t)kernel_threadd_main);
        // // ebx
        // push_stack(esp, 0);
        // // edi
        // push_stack(esp, 0);
        // // esi
        // push_stack(esp, 0);
        // // ebp
        // push_stack(esp, 0);
        // // eflags
        // push_stack(esp, 0x200);
        // // original esp
        // push_stack(esp, old_esp);

        // kernel::task::dispatcher::enqueue(&thd);
    }
}

process& proclist::real_emplace(pid_t pid, pid_t ppid)
{
    auto [ iter, inserted ] = m_procs.try_emplace(pid, pid, ppid);
    assert(inserted);

    return iter->second;
}

void proclist::kill(pid_t pid, int exit_code)
{
    auto& proc = this->find(pid);

    // put all threads into sleep
    for (auto& thd : proc.thds)
        thd.set_attr(kernel::task::thread::ZOMBIE);

    // write back mmap'ped files and close them
    proc.files.close_all();

    // unmap all user memory areas
    proc.mms.clear();

    // init should never exit
    if (proc.ppid == 0) {
        kmsg("kernel panic: init exited!");
        freeze();
    }

    // make child processes orphans (children of init)
    this->make_children_orphans(pid);

    proc.attr.zombie = 1;

    // notify parent process and init
    auto& parent = this->find(proc.ppid);
    auto& init = this->find(1);

    bool flag = false;
    if (1) {
        lock_guard_irq lck(init.mtx_waitprocs);

        if (1) {
            lock_guard_irq lck(proc.mtx_waitprocs);

            for (const auto& item : proc.waitprocs) {
                if (WIFSTOPPED(item.code) || WIFCONTINUED(item.code))
                    continue;

                init.waitprocs.push_back(item);
                flag = true;
            }

            proc.waitprocs.clear();
        }
    }

    if (flag)
        init.waitlist.notify_all();

    if (1) {
        lock_guard_irq lck(parent.mtx_waitprocs);
        parent.waitprocs.push_back({ pid, exit_code });
    }

    parent.waitlist.notify_all();
}

static void release_kinit()
{
    // free .kinit
    using namespace kernel::mem::paging;
    extern uintptr_t KINIT_START_ADDR, KINIT_END_ADDR, KINIT_PAGES;

    auto range = vaddr_range{KERNEL_PAGE_TABLE_ADDR,
        KINIT_START_ADDR, KINIT_END_ADDR, true};

    for (auto pte : range)
        pte.clear();

    create_zone(0x2000, 0x2000 + 0x1000 * KINIT_PAGES);
}

void NORETURN _kernel_init(void)
{
    release_kinit();

    asm volatile("sti");

    // ------------------------------------------
    // interrupt enabled
    // ------------------------------------------

    // load kmods
    for (auto loader = kernel::module::KMOD_LOADERS_START; *loader; ++loader) {
        auto* mod = (*loader)();
        if (!mod)
            continue;

        if (auto ret = insmod(mod); ret == kernel::module::MODULE_SUCCESS)
            continue;

        kmsgf("[kernel] An error occured while loading \"%s\"", mod->name);
    }

    // mount fat32 /mnt directory
    // TODO: parse kernel parameters
    if (1) {
        auto* mount_point = fs::vfs_open(*fs::fs_root, types::path{"/mnt"});
        if (!mount_point) {
            int ret = fs::vfs_mkdir(fs::fs_root, "mnt", 0755);
            assert(ret == 0);

            mount_point = fs::vfs_open(*fs::fs_root, types::path{"/mnt"});
        }

        assert(mount_point);

        int ret = fs::fs_root->ind->fs->mount(mount_point, "/dev/sda", "/mnt",
                "fat32", MS_RDONLY | MS_NOATIME | MS_NODEV | MS_NOSUID, "ro,nodev");
        assert(ret == 0);
    }

    current_process->attr.system = 0;
    current_thread->attr |= kernel::task::thread::SYSTEM;

    types::elf::elf32_load_data d{
        .exec_dent{},
        .argv{ "/mnt/busybox", "sh", "/mnt/initsh" },
        .envp{ "LANG=C", "HOME=/root", "PATH=/mnt", "PWD=/" },
        .ip{}, .sp{}
    };

    d.exec_dent = fs::vfs_open(*fs::fs_root, types::path{d.argv[0].c_str()});
    if (!d.exec_dent) {
        kmsg("kernel panic: init not found!");
        freeze();
    }

    int ret = types::elf::elf32_load(d);
    assert(ret == 0);

    asm volatile(
        "mov $0x23, %%ax\n"
        "mov %%ax, %%ds\n"
        "mov %%ax, %%es\n"
        "mov %%ax, %%fs\n"
        "mov %%ax, %%gs\n"

        "push $0x23\n"
        "push %0\n"
        "push $0x200\n"
        "push $0x1b\n"
        "push %1\n"

        "iretq\n"
        : : "g"(d.sp), "g"(d.ip) : "eax", "memory");

    freeze();
}

void k_new_thread(void (*func)(void*), void* data)
{
    lock_guard lck(kthreadd_mtx);
    kthreadd_new_thd_func = func;
    kthreadd_new_thd_data = data;
}

SECTION(".text.kinit")
void NORETURN init_scheduler(void)
{
    procs = new proclist;

    asm volatile(
        "mov %0, %%rsp\n"
        "sub $16, %%rsp\n"
        "mov %=f, %%rbx\n"
        "mov %%rbx, 8(%%rsp)\n" // return address
        "xor %%rbx, %%rbx\n"
        "mov %%rbx, (%%rsp)\n"  // previous rbp
        "mov %%rsp, %%rbp\n"

        "push %1\n"

        "mov $0x10, %%ax\n"
        "mov %%ax, %%ss\n"
        "mov %%ax, %%ds\n"
        "mov %%ax, %%es\n"
        "mov %%ax, %%fs\n"
        "mov %%ax, %%gs\n"

        "push $0x0\n"
        "popf\n"

        "ret\n"

        "%=:\n"
        "ud2"
        :
        : "a"(current_thread->kstack.sp), "c"(_kernel_init)
        : "memory");

    freeze();
}

extern "C" void asm_ctx_switch(uint32_t** curr_esp, uint32_t** next_esp);
bool schedule()
{
    if (kernel::async::preempt_count() != 0)
        return true;

    auto* next_thd = kernel::task::dispatcher::next();
    process* proc = nullptr;
    kernel::task::thread* curr_thd = nullptr;

    if (current_thread == next_thd)
        goto _end;

    proc = &procs->find(next_thd->owner);
    if (current_process != proc) {
        proc->mms.switch_pd();
        current_process = proc;
    }

    curr_thd = current_thread;

    freeze();
    // TODO: LONG MODE
    // current_thread = next_thd;
    // tss.esp0 = (uint32_t)next_thd->kstack.esp;

    // next_thd->load_thread_area();

    // asm_ctx_switch(&curr_thd->kstack.esp, &next_thd->kstack.esp);
    // tss.esp0 = (uint32_t)curr_thd->kstack.esp;

_end:

    return current_thread->signals.pending_signal() == 0;
}

void NORETURN schedule_noreturn(void)
{
    schedule();
    freeze();
}

void NORETURN freeze(void)
{
    for (;;)
        asm volatile("cli\n\thlt");
}

void NORETURN kill_current(int signo)
{
    procs->kill(current_process->pid,
        (signo + 128) << 8 | (signo & 0xff));
    schedule_noreturn();
}
