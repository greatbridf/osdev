#include <asm/port_io.h>
#include <asm/sys.h>
#include <fs/fat.hpp>
#include <kernel/hw/ata.hpp>
#include <kernel/interrupt.h>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/stdio.h>
#include <kernel/tty.h>
#include <kernel/vfs.hpp>
#include <kernel_main.h>
#include <types/allocator.hpp>
#include <types/assert.h>
#include <types/elf.hpp>
#include <types/hash_map.hpp>
#include <types/list.hpp>
#include <types/lock.hpp>
#include <types/size.h>
#include <types/status.h>
#include <types/stdint.h>
#include <types/types.h>

static bool is_scheduler_ready;
static types::list<process>* processes;
static typename types::hash_map<pid_t, types::list<process>::iterator_type, types::linux_hasher<pid_t>>* idx_processes;
static types::list<thread*>* ready_thds;
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

process::process(process&& val)
    : mms(types::move(val.mms))
    , thds(types::move(val.thds))
    , wait_lst(types::move(val.wait_lst))
    , pid(val.pid)
    , ppid(val.ppid)
{
    if (current_process == &val)
        current_process = this;

    attr.system = val.attr.system;

    for (auto& item : thds)
        item.owner = this;

    val.attr.system = 0;
}

process::process(const process& val, const thread& main_thd)
    : mms(*kernel_mms)
    , attr { .system = val.attr.system }
    , pid { process::alloc_pid() }
    , ppid { val.pid }
{
    auto iter_thd = thds.emplace_back(main_thd);
    iter_thd->owner = this;

    for (auto& area : val.mms) {
        if (area.is_ident())
            continue;

        mms.mirror_area(area);
    }
}

process::process(void)
    : mms(*kernel_mms)
    , attr { .system = 1 }
    , pid { process::alloc_pid() }
    , ppid { 1 }
{
    auto thd = thds.emplace_back(this, true);

    add_to_ready_list(thd.ptr());
}

process::process(void (*func)(void), pid_t _ppid)
    : mms(*kernel_mms)
    , attr { .system = 1 }
    , pid { process::alloc_pid() }
    , ppid { _ppid }
{
    auto thd = thds.emplace_back(this, true);

    add_to_ready_list(thd.ptr());

    auto* esp = &thd->esp;

    // return(start) address
    push_stack(esp, (uint32_t)func);
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
}

process::~process()
{
    for (auto iter = thds.begin(); iter != thds.end(); ++iter)
        remove_from_ready_list(iter.ptr());
}

inline void NORETURN _noreturn_crash(void)
{
    for (;;)
        assert(false);
}

extern "C" void NORETURN go_kernel(uint32_t* kstack, void (*k_main)(void));
extern "C" void NORETURN go_user(void* eip, uint32_t* esp);

void kernel_threadd_main(void)
{
    tty_print(console, "kernel thread daemon started\n");

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
    {
        kernel::no_irq_guard grd;

        add_to_process_list(process { kernel_threadd_main, 1 });
    }
    hw::init_ata();

    // TODO: parse kernel parameters
    auto* _new_fs = fs::register_fs(types::kernel_allocator_new<fs::fat::fat32>(fs::vfs_open("/dev/hda1")->ind));
    int ret = fs::fs_root->ind->fs->mount(fs::vfs_open("/mnt"), _new_fs);
    assert_likely(ret == GB_OK);

    current_process->attr.system = 0;
    current_thread->attr.system = 0;

    const char* argv[] = { "/mnt/INIT.ELF", nullptr };

    types::elf::elf32_load_data d;
    d.exec = "/mnt/INIT.ELF";
    d.argv = argv;
    d.system = false;

    assert(types::elf::elf32_load(&d) == GB_OK);

    is_scheduler_ready = true;

    go_user(d.eip, d.sp);
}

void k_new_thread(void (*func)(void*), void* data)
{
    types::lock_guard lck(kthreadd_mtx);
    kthreadd_new_thd_func = func;
    kthreadd_new_thd_data = data;
}

void NORETURN init_scheduler()
{
    processes = types::kernel_allocator_pnew(processes);
    ready_thds = types::kernel_allocator_pnew(ready_thds);
    idx_processes = types::kernel_allocator_pnew(idx_processes);
    idx_child_processes = types::kernel_allocator_pnew(idx_child_processes);

    auto pid = add_to_process_list(process {});
    auto init = findproc(pid);

    // we need interrupts enabled for cow mapping so now we disable it
    // in case timer interrupt mess things up
    asm_cli();

    current_process = init;
    current_thread = init->thds.begin().ptr();

    tss.ss0 = KERNEL_DATA_SEGMENT;
    tss.esp0 = current_thread->kstack;

    asm_switch_pd(current_process->mms.m_pd);

    go_kernel(current_thread->esp, _kernel_init);
}

pid_t add_to_process_list(process&& proc)
{
    auto iter = processes->emplace_back(types::move(proc));
    idx_processes->insert(iter->pid, iter);

    auto children = idx_child_processes->find(iter->ppid);
    if (!children) {
        idx_child_processes->insert(iter->ppid, {});
        children = idx_child_processes->find(iter->ppid);
    }

    children->value.push_back(iter->pid);

    return iter->pid;
}

void remove_from_process_list(pid_t pid)
{
    auto proc_iter = idx_processes->find(pid);
    auto ppid = proc_iter->value->ppid;

    auto& parent_children = idx_child_processes->find(ppid)->value;

    auto i = parent_children.find(pid);
    parent_children.erase(i);

    processes->erase(proc_iter->value);
    idx_processes->remove(proc_iter);
}

void add_to_ready_list(thread* thd)
{
    ready_thds->push_back(thd);
}

void remove_from_ready_list(thread* thd)
{
    auto iter = ready_thds->find(thd);
    while (iter != ready_thds->end()) {
        ready_thds->erase(iter);
        iter = ready_thds->find(thd);
    }
}

types::list<thread*>::iterator_type query_next_thread(void)
{
    auto iter_thd = ready_thds->begin();
    while (!((*iter_thd)->attr.ready))
        iter_thd = ready_thds->erase(iter_thd);
    return iter_thd;
}

process* findproc(pid_t pid)
{
    return idx_processes->find(pid)->value.ptr();
}

extern "C" void asm_ctx_switch(uint32_t** curr_esp, uint32_t* next_esp);
void schedule()
{
    if (unlikely(!is_scheduler_ready))
        return;

    auto iter_thd = query_next_thread();
    auto thd = *iter_thd;

    if (current_thread == thd) {
        next_task(iter_thd);
        return;
    }

    process* proc = thd->owner;
    if (current_process != proc) {
        asm_switch_pd(proc->mms.m_pd);
        current_process = proc;
    }

    auto* curr_thd = current_thread;

    current_thread = thd;
    tss.esp0 = current_thread->kstack;
    next_task(iter_thd);

    asm_ctx_switch(&curr_thd->esp, thd->esp);
}
