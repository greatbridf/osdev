#include <asm/port_io.h>
#include <asm/sys.h>
#include <assert.h>
#include <fs/fat.hpp>
#include <kernel/hw/ata.hpp>
#include <kernel/interrupt.h>
#include <kernel/log.hpp>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/signal.hpp>
#include <kernel/vfs.hpp>
#include <stdint.h>
#include <stdio.h>
#include <types/allocator.hpp>
#include <types/bitmap.h>
#include <types/cplusplus.hpp>
#include <types/elf.hpp>
#include <types/hash_map.hpp>
#include <types/list.hpp>
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

namespace __thd {
inline uint8_t __kstack_bmp[(0x1000000 - 0xc00000) / 0x2000 / 8];
inline int __allocated;
} // namespace __thd

void thread::alloc_kstack(void)
{
    for (int i = 0; i < __thd::__allocated; ++i) {
        if (bm_test(__thd::__kstack_bmp, i) == 0) {
            pkstack = 0xffc00000 + THREAD_KERNEL_STACK_SIZE * (i + 1);
            esp = reinterpret_cast<uint32_t*>(pkstack);

            bm_set(__thd::__kstack_bmp, i);
            return;
        }
    }

    // kernel stack pt is at page#0x00005
    kernel::paccess pa(0x00005);
    auto pt = (pt_t)pa.ptr();
    assert(pt);
    pte_t* pte = *pt + __thd::__allocated * 2;

    pte[0].v = 0x3;
    pte[0].in.page = __alloc_raw_page();
    pte[1].v = 0x3;
    pte[1].in.page = __alloc_raw_page();

    pkstack = 0xffc00000 + THREAD_KERNEL_STACK_SIZE * (__thd::__allocated + 1);
    esp = reinterpret_cast<uint32_t*>(pkstack);

    bm_set(__thd::__kstack_bmp, __thd::__allocated);
    ++__thd::__allocated;
}

void thread::free_kstack(uint32_t p)
{
    p -= 0xffc00000;
    p /= THREAD_KERNEL_STACK_SIZE;
    p -= 1;
    bm_clear(__thd::__kstack_bmp, p);
}

process::process(process&& val)
    : mms(types::move(val.mms))
    , thds { types::move(val.thds), this }
    , attr { val.attr }
    , files(types::move(val.files))
    , pwd(types::move(val.pwd))
    , pid(val.pid)
    , ppid(val.ppid)
    , pgid(val.pgid)
    , sid(val.sid)
    , start_brk(val.start_brk)
    , brk(val.brk)
{
    if (current_process == &val)
        current_process = this;
}

process::process(const process& parent)
    : process { parent.pid,
        parent.is_system(),
        types::string<>(parent.pwd),
        kernel::signal_list(parent.signals) }
{
    this->pgid = parent.pgid;
    this->sid = parent.sid;

    this->start_brk = parent.start_brk;
    this->brk = parent.brk;

    for (auto& area : parent.mms) {
        if (area.is_kernel_space() || area.attr.in.system)
            continue;

        mms.mirror_area(area);
    }

    this->files.dup_all(parent.files);
}

process::process(pid_t _ppid,
    bool _system,
    types::string<>&& path,
    kernel::signal_list&& _sigs)
    : mms(*kernel_mms)
    , attr { .system = _system }
    , pwd { types::move(path) }
    , signals(types::move(_sigs))
    , pid { process::alloc_pid() }
    , ppid { _ppid }
    , pgid { 0 }
    , sid { 0 }
    , start_brk(nullptr)
    , brk(nullptr)
{
}

void proclist::kill(pid_t pid, int exit_code)
{
    process* proc = this->find(pid);

    // remove threads from ready list
    for (auto& thd : proc->thds.underlying_list()) {
        thd.attr.ready = 0;
        readythds->remove_all(&thd);
    }

    // write back mmap'ped files and close them
    proc->files.close_all();

    // unmap all user memory areas
    proc->mms.clear_user();

    // init should never exit
    if (proc->ppid == 0) {
        console->print("kernel panic: init exited!\n");
        assert(false);
    }

    // make child processes orphans (children of init)
    this->make_children_orphans(pid);

    proc->attr.zombie = 1;

    // notify parent process and init
    auto* parent = this->find(proc->ppid);
    auto* init = this->find(1);

    bool flag = false;
    {
        auto& mtx = init->cv_wait.mtx();
        types::lock_guard lck(mtx);

        {
            auto& mtx = proc->cv_wait.mtx();
            types::lock_guard lck(mtx);

            for (const auto& item : proc->waitlist) {
                init->waitlist.push_back(item);
                flag = true;
            }

            proc->waitlist.clear();
        }
    }
    if (flag)
        init->cv_wait.notify();

    {
        auto& mtx = parent->cv_wait.mtx();
        types::lock_guard lck(mtx);
        parent->waitlist.push_back({ pid, exit_code });
    }
    parent->cv_wait.notify();
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
                func = kthreadd_new_thd_func;
                data = kthreadd_new_thd_data;

                kthreadd_new_thd_func = nullptr;
                kthreadd_new_thd_data = nullptr;
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

void NORETURN _kernel_init(void)
{
    // pid 2 is kernel thread daemon
    auto* proc = &procs->emplace(1)->value;

    // create thread
    thread thd(proc, true);

    auto* esp = &thd.esp;

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

    readythds->push(&proc->thds.Emplace(types::move(thd)));

    // ------------------------------------------

    asm_sti();

    hw::init_ata();

    // TODO: parse kernel parameters
    auto* drive = fs::vfs_open("/dev/hda1");
    assert(drive);
    auto* _new_fs = fs::register_fs(new fs::fat::fat32(drive->ind));
    auto* mnt = fs::vfs_open("/mnt");
    assert(mnt);
    int ret = fs::fs_root->ind->fs->mount(mnt, _new_fs);
    assert(ret == GB_OK);

    current_process->attr.system = 0;
    current_thread->attr.system = 0;

    const char* argv[] = { "/mnt/init", "/mnt/sh", nullptr };
    const char* envp[] = { nullptr };

    types::elf::elf32_load_data d;
    auto* dent = fs::vfs_open("/mnt/init");
    assert(dent && dent->ind);
    d.exec = dent->ind;
    d.argv = argv;
    d.envp = envp;
    d.system = false;

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

void NORETURN init_scheduler(void)
{
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

    procs = new proclist;
    readythds = new readyqueue;

    process::filearr::init_global_file_container();

    // init process has no parent
    auto* init = &procs->emplace(0)->value;
    init->files.open("/dev/console", O_RDONLY);
    init->files.open("/dev/console", O_WRONLY);
    init->files.open("/dev/console", O_WRONLY);

    // we need interrupts enabled for cow mapping so now we disable it
    // in case timer interrupt mess things up
    asm_cli();

    current_process = init;
    current_thread = &init->thds.Emplace(init, true);
    readythds->push(current_thread);

    tss.ss0 = KERNEL_DATA_SEGMENT;
    tss.esp0 = current_thread->pkstack;

    asm_switch_pd(current_process->mms.m_pd);

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

extern "C" void asm_ctx_switch(uint32_t** curr_esp, uint32_t* next_esp);
bool schedule()
{
    auto thd = readythds->query();
    process* proc = nullptr;
    thread* curr_thd = nullptr;

    if (current_thread == thd)
        goto _end;

    proc = thd->owner;
    if (current_process != proc) {
        asm_switch_pd(proc->mms.m_pd);
        current_process = proc;
    }

    curr_thd = current_thread;

    current_thread = thd;
    tss.esp0 = current_thread->pkstack;

    asm_ctx_switch(&curr_thd->esp, thd->esp);

_end:
    return current_process->signals.empty();
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

void NORETURN kill_current(int exit_code)
{
    procs->kill(current_process->pid, exit_code);
    schedule_noreturn();
}

void check_signal()
{
    switch (current_process->signals.pop()) {
    case kernel::SIGINT:
    case kernel::SIGQUIT:
    case kernel::SIGSTOP:
        procs->get_ctrl_tty(current_process->pid)->clear_read_buf();
        kill_current(-1);
        break;
    case kernel::SIGPIPE:
        kill_current(-1);
        break;
    case 0:
        break;
    }
}
