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
using kernel::async::lock_guard;

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

process::process(const process& parent, pid_t pid)
    : mms { parent.mms }, attr { parent.attr } , files { parent.files.copy() }
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
    kmsg("[kernel] kthread daemon started");

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
        }
        // TODO: sleep here to wait for new_kernel_thread event
        asm volatile("hlt");
    }
}

static inline void __spawn(kernel::task::thread& thd, uintptr_t entry)
{
    auto prev_sp = thd.kstack.sp;

    // return(start) address
    thd.kstack.pushq(entry);
    thd.kstack.pushq(0x200);       // flags
    thd.kstack.pushq(0);           // r15
    thd.kstack.pushq(0);           // r14
    thd.kstack.pushq(0);           // r13
    thd.kstack.pushq(0);           // r12
    thd.kstack.pushq(0);           // rbp
    thd.kstack.pushq(0);           // rbx
    thd.kstack.pushq(0);           // 0 for alignment
    thd.kstack.pushq(prev_sp);     // previous sp
}

SECTION(".text.kinit")
proclist::proclist()
{
    // init process has no parent
    auto& init = real_emplace(1, 0);
    assert(init.pid == 1 && init.ppid == 0);

    auto thd = init.thds.begin();
    thd->name.assign("[kernel init]");

    current_process = &init;
    current_thread = &thd;

    kernel::task::dispatcher::enqueue(current_thread);

    current_thread->kstack.load_interrupt_stack();
    current_process->mms.switch_pd();

    if (1) {
        // pid 0 is kernel thread daemon
        auto& proc = real_emplace(0, 0);
        assert(proc.pid == 0 && proc.ppid == 0);

        // create thread
        auto thd = proc.thds.begin();
        thd->name.assign("[kernel thread daemon]");

        __spawn(*thd, (uintptr_t)kernel_threadd_main);

        kernel::task::dispatcher::setup_idle(&thd);
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

    // init should never exit
    if (proc.ppid == 0) {
        kmsg("kernel panic: init exited!");
        freeze();
    }

    kernel::async::preempt_disable();

    // put all threads into sleep
    for (auto& thd : proc.thds)
        thd.set_attr(kernel::task::thread::ZOMBIE);

    // TODO: CHANGE THIS
    //       files should only be closed when this is the last thread
    //
    // write back mmap'ped files and close them
    proc.files.clear();

    // unmap all user memory areas
    proc.mms.clear();

    // make child processes orphans (children of init)
    this->make_children_orphans(pid);

    proc.attr.zombie = 1;

    // notify parent process and init
    auto& parent = this->find(proc.ppid);
    auto& init = this->find(1);

    bool flag = false;
    if (1) {
        lock_guard lck(init.mtx_waitprocs);

        if (1) {
            lock_guard lck(proc.mtx_waitprocs);

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
        lock_guard lck(parent.mtx_waitprocs);
        parent.waitprocs.push_back({ pid, exit_code });
    }

    parent.waitlist.notify_all();

    kernel::async::preempt_enable();
}

static void release_kinit()
{
    // free .kinit
    using namespace kernel::mem::paging;
    extern uintptr_t volatile KINIT_START_ADDR, KINIT_END_ADDR, KINIT_PAGES;

    std::size_t pages = KINIT_PAGES;
    auto range = vaddr_range{KERNEL_PAGE_TABLE_ADDR,
        KINIT_START_ADDR, KINIT_END_ADDR, true};
    for (auto pte : range)
        pte.clear();

    create_zone(0x2000, 0x2000 + 0x1000 * pages);
}

void NORETURN _kernel_init(kernel::mem::paging::pfn_t kernel_stack_pfn)
{
    kernel::mem::paging::free_pages(kernel_stack_pfn, 9);
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
    current_thread->attr &= ~kernel::task::thread::SYSTEM;

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

    int ds = 0x33, cs = 0x2b;

    asm volatile(
        "mov %0, %%rax\n"
        "mov %%ax, %%ds\n"
        "mov %%ax, %%es\n"
        "mov %%ax, %%fs\n"
        "mov %%ax, %%gs\n"

        "push %%rax\n"
        "push %2\n"
        "push $0x200\n"
        "push %1\n"
        "push %3\n"

        "iretq\n"
        : : "g"(ds), "g"(cs), "g"(d.sp),
            "g"(d.ip) : "eax", "memory");

    freeze();
}

void k_new_thread(void (*func)(void*), void* data)
{
    lock_guard lck(kthreadd_mtx);
    kthreadd_new_thd_func = func;
    kthreadd_new_thd_data = data;
}

SECTION(".text.kinit")
void NORETURN init_scheduler(kernel::mem::paging::pfn_t kernel_stack_pfn)
{
    procs = new proclist;

    asm volatile(
        "mov %2, %%rdi\n"
        "mov %0, %%rsp\n"
        "sub $24, %%rsp\n"
        "mov %=f, %%rbx\n"
        "mov %%rbx, (%%rsp)\n"   // return address
        "mov %%rbx, 16(%%rsp)\n" // previous frame return address
        "xor %%rbx, %%rbx\n"
        "mov %%rbx, 8(%%rsp)\n"  // previous frame rbp
        "mov %%rsp, %%rbp\n"     // current frame rbp

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
        : "a"(current_thread->kstack.sp), "c"(_kernel_init), "g"(kernel_stack_pfn)
        : "memory");

    freeze();
}

extern "C" void asm_ctx_switch(uintptr_t* curr_sp, uintptr_t* next_sp);

extern "C" void after_ctx_switch()
{
    current_thread->kstack.load_interrupt_stack();
    current_thread->load_thread_area32();
}

bool _schedule()
{
    auto* next_thd = kernel::task::dispatcher::next();

    if (current_thread != next_thd) {
        auto* proc = &procs->find(next_thd->owner);
        if (current_process != proc) {
            proc->mms.switch_pd();
            current_process = proc;
        }

        auto* curr_thd = current_thread;
        current_thread = next_thd;

        asm_ctx_switch(&curr_thd->kstack.sp, &next_thd->kstack.sp);
    }

    return current_thread->signals.pending_signal() == 0;
}

bool schedule()
{
    if (kernel::async::preempt_count() != 0)
        return true;

    return _schedule();
}

void NORETURN schedule_noreturn(void)
{
    _schedule();
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
