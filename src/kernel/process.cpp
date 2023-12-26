#include <memory>
#include <utility>

#include <bits/alltypes.h>

#include <asm/port_io.h>
#include <asm/sys.h>
#include <assert.h>
#include <fs/fat.hpp>
#include <kernel/interrupt.h>
#include <kernel/log.hpp>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/module.hpp>
#include <kernel/process.hpp>
#include <kernel/signal.hpp>
#include <kernel/vfs.hpp>
#include <stdint.h>
#include <stdio.h>
#include <types/allocator.hpp>
#include <types/bitmap.hpp>
#include <types/cplusplus.hpp>
#include <types/elf.hpp>
#include <types/hash_map.hpp>
#include <types/lock.hpp>
#include <types/size.h>
#include <types/status.h>
#include <types/types.h>

static void (*volatile kthreadd_new_thd_func)(void*);
static void* volatile kthreadd_new_thd_data;
static types::mutex kthreadd_mtx;

namespace kernel {

struct no_irq_guard {
    explicit no_irq_guard()
    {
        asm_cli();
    }

    no_irq_guard(const no_irq_guard&) = delete;
    no_irq_guard& operator=(const no_irq_guard&) = delete;

    ~no_irq_guard()
    {
        asm_sti();
    }
};

} // namespace kernel

static types::bitmap* pkstack_bmp;

void kernel::tasks::thread::alloc_kstack(void)
{
    static int __allocated;
    if (!pkstack_bmp)
        pkstack_bmp = new types::bitmap((0x1000000 - 0xc00000) / THREAD_KERNEL_STACK_SIZE);

    for (int i = 0; i < __allocated; ++i) {
        if (pkstack_bmp->test(i) == 0) {
            pkstack = 0xffc00000 + THREAD_KERNEL_STACK_SIZE * (i + 1);
            esp = reinterpret_cast<uint32_t*>(pkstack);

            pkstack_bmp->set(i);
            return;
        }
    }

    // kernel stack pt is at page#0x00005
    kernel::paccess pa(0x00005);
    auto pt = (pt_t)pa.ptr();
    assert(pt);

    auto cnt = THREAD_KERNEL_STACK_SIZE / PAGE_SIZE;
    pte_t* pte = *pt + __allocated * cnt;

    for (uint32_t i = 0; i < cnt; ++i) {
        pte[i].v = 0x3;
        pte[i].in.page = __alloc_raw_page();
    }

    pkstack = 0xffc00000 + THREAD_KERNEL_STACK_SIZE * (__allocated + 1);
    esp = reinterpret_cast<uint32_t*>(pkstack);

    pkstack_bmp->set(__allocated);
    ++__allocated;
}

void kernel::tasks::thread::free_kstack(uint32_t p)
{
    p -= 0xffc00000;
    p /= THREAD_KERNEL_STACK_SIZE;
    p -= 1;
    pkstack_bmp->clear(p);
}

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
    this->arr.emplace(new_fd, iter->second);
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
            if (ret != GB_OK)
                return ret;
            dentry = fs::vfs_open(*current.root, filepath);
            assert(dentry);
        } else {
            // file already exists
            if (flags & O_EXCL)
                return -EEXIST;

            if (flags & O_TRUNC) {
                // TODO: truncate file
            }
        }
    } else {
        if (!dentry)
            return -ENOENT;
    }

    // check whether dentry is a file if O_DIRECTORY is set
    if (flags & O_DIRECTORY) {
        if (!S_ISDIR(dentry->ind->mode))
            return -ENOTDIR;
    } else {
        if (S_ISDIR(dentry->ind->mode) && (flags & (O_WRONLY | O_RDWR)))
            return -EISDIR;
    }

    int fd = next_fd();
    auto [ _, inserted ] = arr.emplace(fd, fditem {
        flags, std::shared_ptr<fs::file> {
            new fs::regular_file(dentry->parent, {
                .read = !(flags & O_WRONLY),
                .write = !!(flags & (O_WRONLY | O_RDWR)),
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
    , pwd { "/" } , pid { pid } , ppid { ppid } { }

using kernel::tasks::thread;
using signo_type = kernel::signal_list::signo_type;

void process::send_signal(signo_type signal)
{
    for (auto& thd : thds)
        thd.send_signal(signal);
}

void thread::sleep()
{
    attr.ready = 0;
    readythds->remove_all(this);
}

void thread::wakeup()
{
    attr.ready = 1;
    readythds->push(this);
}

void thread::send_signal(signo_type signal)
{
    if (signals.raise(signal))
        this->wakeup();
}

void proclist::kill(pid_t pid, int exit_code)
{
    auto& proc = this->find(pid);

    // put all threads into sleep
    for (auto& thd : proc.thds)
        thd.sleep();

    // if current process is connected to a tty
    // clear its read buffer
    // TODO: make tty line discipline handle this
    tty* ctrl_tty = current_process->control_tty;
    if (ctrl_tty)
        ctrl_tty->clear_read_buf();

    // write back mmap'ped files and close them
    proc.files.close_all();

    // unmap all user memory areas
    proc.mms.clear_user();

    // init should never exit
    if (proc.ppid == 0) {
        console->print("kernel panic: init exited!\n");
        freeze();
    }

    // make child processes orphans (children of init)
    this->make_children_orphans(pid);

    proc.attr.zombie = 1;

    // notify parent process and init
    auto& parent = this->find(proc.ppid);
    auto& init = this->find(1);

    bool flag = false;
    {
        auto& mtx = init.cv_wait.mtx();
        types::lock_guard lck(mtx);

        {
            auto& mtx = proc.cv_wait.mtx();
            types::lock_guard lck(mtx);

            for (const auto& item : proc.waitlist) {
                init.waitlist.push_back(item);
                flag = true;
            }

            proc.waitlist.clear();
        }
    }
    if (flag)
        init.cv_wait.notify();

    {
        auto& mtx = parent.cv_wait.mtx();
        types::lock_guard lck(mtx);
        parent.waitlist.push_back({ pid, exit_code });
    }
    parent.cv_wait.notify();
}

void kernel_threadd_main(void)
{
    kmsg("kernel thread daemon started\n");

    for (;;) {
        if (kthreadd_new_thd_func) {
            void (*func)(void*) = nullptr;
            void* data = nullptr;

            {
                types::lock_guard lck(kthreadd_mtx);
                func = std::exchange(kthreadd_new_thd_func, nullptr);
                data = std::exchange(kthreadd_new_thd_data, nullptr);
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
        asm_hlt();
    }
}

static void release_kinit()
{
    extern char __stage1_start[];
    extern char __kinit_end[];

    kernel::paccess pa(EARLY_KERNEL_PD_PAGE);
    auto pd = (pd_t)pa.ptr();
    assert(pd);
    (*pd)[0].v = 0;

    // free pt#0
    __free_raw_page(0x00002);

    // free .stage1 and .kinit
    for (uint32_t i = ((uint32_t)__stage1_start >> 12);
            i < ((uint32_t)__kinit_end >> 12); ++i) {
        __free_raw_page(i);
    }
}

static void create_kthreadd_process()
{
    // pid 2 is kernel thread daemon
    auto& proc = procs->emplace(1);
    assert(proc.pid == 2);

    // create thread
    auto [ iter_thd, inserted] =
        proc.thds.emplace("[kernel thread daemon]", proc.pid);
    assert(inserted);
    auto& thd = *iter_thd;

    auto* esp = &thd.esp;
    auto old_esp = (uint32_t)thd.esp;

    // return(start) address
    push_stack(esp, (uint32_t)kernel_threadd_main);
    // ebx
    push_stack(esp, 0);
    // edi
    push_stack(esp, 0);
    // esi
    push_stack(esp, 0);
    // ebp
    push_stack(esp, 0);
    // eflags
    push_stack(esp, 0x200);
    // original esp
    push_stack(esp, old_esp);

    readythds->push(&thd);
}

void NORETURN _kernel_init(void)
{
    create_kthreadd_process();

    release_kinit();

    asm_sti();

    // ------------------------------------------
    // interrupt enabled
    // ------------------------------------------

    // load kmods
    for (auto loader = kernel::module::kmod_loaders_start; *loader; ++loader) {
        auto* mod = (*loader)();
        if (!mod)
            continue;

        auto ret = insmod(mod);
        if (ret == kernel::module::MODULE_SUCCESS)
            continue;

        char buf[256];
        snprintf(buf, sizeof(buf),
            "[kernel] An error occured while loading \"%s\"\n", mod->name);
        kmsg(buf);
    }

    // TODO: parse kernel parameters
    auto* drive = fs::vfs_open(*fs::fs_root, "/dev/sda1");
    assert(drive);
    auto* _new_fs = fs::register_fs(new fs::fat::fat32(drive->ind));
    auto* mnt = fs::vfs_open(*fs::fs_root, "/mnt");
    assert(mnt);
    int ret = fs::fs_root->ind->fs->mount(mnt, _new_fs);
    assert(ret == GB_OK);

    current_process->attr.system = 0;
    current_thread->attr.system = 0;

    const char* argv[] = { "/mnt/init", "/mnt/sh", nullptr };
    const char* envp[] = { "LANG=C", "HOME=/", nullptr };

    types::elf::elf32_load_data d;
    d.argv = argv;
    d.envp = envp;
    d.system = false;

    d.exec_dent = fs::vfs_open(*fs::fs_root, "/mnt/init");
    if (!d.exec_dent) {
        console->print("kernel panic: init not found!\n");
        freeze();
    }

    ret = types::elf::elf32_load(&d);
    assert(ret == GB_OK);

    asm volatile(
        "movw $0x23, %%ax\n"
        "movw %%ax, %%ds\n"
        "movw %%ax, %%es\n"
        "movw %%ax, %%fs\n"
        "movw %%ax, %%gs\n"

        "pushl $0x23\n"
        "pushl %0\n"
        "pushl $0x200\n"
        "pushl $0x1b\n"
        "pushl %1\n"

        "iret\n"
        :
        : "c"(d.sp), "d"(d.eip)
        : "eax", "memory");

    freeze();
}

void k_new_thread(void (*func)(void*), void* data)
{
    types::lock_guard lck(kthreadd_mtx);
    kthreadd_new_thd_func = func;
    kthreadd_new_thd_data = data;
}

SECTION(".text.kinit")
void NORETURN init_scheduler(void)
{
    procs = new proclist;
    readythds = new readyqueue;

    // init process has no parent
    auto& init = procs->emplace(0);
    assert(init.pid == 1);

    auto [ iter_thd, inserted ] = init.thds.emplace("[kernel init]", init.pid);
    assert(inserted);
    auto& thd = *iter_thd;

    init.files.open(init, "/dev/console", O_RDONLY, 0);
    init.files.open(init, "/dev/console", O_WRONLY, 0);
    init.files.open(init, "/dev/console", O_WRONLY, 0);

    current_process = &init;
    current_thread = &thd;
    readythds->push(current_thread);

    tss.ss0 = KERNEL_DATA_SEGMENT;
    tss.esp0 = current_thread->pkstack;

    current_process->mms.switch_pd();

    asm volatile(
        "movl %0, %%esp\n"
        "pushl %=f\n"
        "pushl %1\n"

        "movw $0x10, %%ax\n"
        "movw %%ax, %%ss\n"
        "movw %%ax, %%ds\n"
        "movw %%ax, %%es\n"
        "movw %%ax, %%fs\n"
        "movw %%ax, %%gs\n"

        "xorl %%ebp, %%ebp\n"
        "xorl %%edx, %%edx\n"

        "pushl $0x0\n"
        "popfl\n"

        "ret\n"

        "%=:\n"
        "ud2"
        :
        : "a"(current_thread->esp), "c"(_kernel_init)
        : "memory");

    freeze();
}

extern "C" void asm_ctx_switch(uint32_t** curr_esp, uint32_t** next_esp);
bool schedule()
{
    auto next_thd = readythds->query();
    process* proc = nullptr;
    kernel::tasks::thread* curr_thd = nullptr;

    if (current_thread == next_thd)
        goto _end;

    proc = &procs->find(next_thd->owner);
    if (current_process != proc) {
        proc->mms.switch_pd();
        current_process = proc;
    }

    curr_thd = current_thread;

    current_thread = next_thd;
    tss.esp0 = (uint32_t)next_thd->esp;

    asm_ctx_switch(&curr_thd->esp, &next_thd->esp);
    tss.esp0 = (uint32_t)curr_thd->esp;

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
    asm_cli();
    asm_hlt();
    for (;;)
        ;
}

void NORETURN kill_current(int signo)
{
    procs->kill(current_process->pid,
        (signo + 128) << 8 | (signo & 0xff));
    schedule_noreturn();
}
