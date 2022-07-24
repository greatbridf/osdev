#include <asm/port_io.h>
#include <asm/sys.h>
#include <fs/fat.hpp>
#include <kernel/hw/ata.hpp>
#include <kernel/interrupt.h>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/stdio.h>
#include <kernel/syscall.hpp>
#include <kernel/tty.h>
#include <kernel/vfs.hpp>
#include <kernel_main.h>
#include <types/allocator.hpp>
#include <types/elf.hpp>
#include <types/hash_map.hpp>
#include <types/list.hpp>
#include <types/lock.hpp>
#include <types/status.h>
#include <types/types.h>

static bool is_scheduler_ready;
static types::list<process>* processes;
static typename types::hash_map<pid_t, types::list<process>::iterator_type, types::linux_hasher<pid_t>>* idx_processes;
static types::list<thread*>* ready_thds;
static pid_t max_pid;
static void (*volatile kthreadd_new_thd_func)(void*);
static void* volatile kthreadd_new_thd_data;
static uint32_t volatile kthreadd_lock = 0;

process::process(process&& val)
    : mms(types::move(val.mms))
    , thds(types::move(val.thds))
    , pid(val.pid)
    , ppid(val.ppid)
{
    if (current_process == &val)
        current_process = this;

    attr.system = val.attr.system;
    k_esp = val.k_esp;

    for (auto& item : thds)
        item.owner = this;

    val.k_esp = nullptr;
    val.attr.system = 0;
}

process::process(const process& val, const thread& main_thd)
    : mms(*kernel_mms)
    , attr { .system = val.attr.system }
    , pid { ++max_pid }
    , ppid { val.pid }
{
    auto iter_thd = thds.emplace_back(main_thd);
    iter_thd->owner = this;

    // TODO: allocate low mem
    k_esp = (void*)to_pp(alloc_n_raw_pages(2));
    memcpy(k_esp, (char*)main_thd.owner->k_esp - THREAD_KERNEL_STACK_SIZE, THREAD_KERNEL_STACK_SIZE);
    k_esp = (char*)k_esp + THREAD_KERNEL_STACK_SIZE;

    if (val.attr.system) {
        auto orig_k_esp = (uint32_t)main_thd.owner->k_esp;

        iter_thd->regs.ebp -= orig_k_esp;
        iter_thd->regs.ebp += (uint32_t)k_esp;

        iter_thd->regs.esp -= orig_k_esp;
        iter_thd->regs.esp += (uint32_t)k_esp;
    } else {
        pd_t pd = alloc_pd();
        memcpy(pd, mms_get_pd(kernel_mms), PAGE_SIZE);

        mms.begin()->pd = pd;

        // skip kernel heap since it's already copied above
        for (auto iter_src = ++val.mms.cbegin(); iter_src != val.mms.cend(); ++iter_src)
            mm::mirror_mm_area(&mms, iter_src.ptr(), pd);
    }
}

process::process(void* start_eip)
    : mms(*kernel_mms)
    , thds {}
    , attr { .system = 1 }
    , pid { ++max_pid }
    , ppid { 1 }
{
    // TODO: allocate low mem
    k_esp = (void*)to_pp(alloc_n_raw_pages(2));
    memset((char*)k_esp, 0x00, THREAD_KERNEL_STACK_SIZE);
    k_esp = (char*)k_esp + THREAD_KERNEL_STACK_SIZE;

    auto thd = thds.emplace_back(thread {
        .eip = start_eip,
        .owner = this,
        .regs {
            .edi {},
            .esi {},
            .ebp = reinterpret_cast<uint32_t>(k_esp),
            .esp = reinterpret_cast<uint32_t>(k_esp),
            .ebx {},
            .edx {},
            .ecx {},
            .eax {},
        },
        .eflags {},
        .attr {
            .system = 1,
            .ready = 1,
            .wait = 0,
        },
    });
    ready_thds->push_back(thd.ptr());
}

void NORETURN _kernel_init(void)
{
    // TODO: parse kernel parameters
    auto* _new_fs = fs::register_fs(types::kernel_allocator_new<fs::fat::fat32>(fs::vfs_open("/dev/hda1")->ind));
    int ret = fs::fs_root->ind->fs->mount(fs::vfs_open("/mnt"), _new_fs);
    if (unlikely(ret != GB_OK))
        syscall(0x03);

    pd_t new_pd = alloc_pd();
    memcpy(new_pd, mms_get_pd(kernel_mms), PAGE_SIZE);

    asm_cli();

    current_process->mms.begin()->pd = new_pd;

    asm_sti();

    interrupt_stack intrpt_stack {};
    intrpt_stack.eflags = 0x200; // STI
    const char* argv[] = { "/mnt/INIT.ELF", nullptr };
    types::elf::elf32_load("/mnt/INIT.ELF", argv, &intrpt_stack, 0);

    asm_cli();
    current_process->attr.system = 0;
    current_thread->attr.system = 0;
    to_user(&intrpt_stack);
}

void kernel_threadd_main(void)
{
    tty_print(console, "kernel thread daemon started\n");

    // fork
    int ret = syscall(0x00);

    // pid 1
    if (ret) {
        hw::init_ata();
        _kernel_init();
        // noreturn
        syscall(0x03);
    }

    for (;;) {
        if (kthreadd_new_thd_func) {
            spin_lock(&kthreadd_lock);
            int return_value = 0;

            void (*func)(void*) = kthreadd_new_thd_func;
            void* data = kthreadd_new_thd_data;
            kthreadd_new_thd_func = nullptr;
            kthreadd_new_thd_data = nullptr;

            spin_unlock(&kthreadd_lock);

            // syscall_fork
            return_value = syscall(0x00);

            if (return_value == 0) {
                // child process
                func(data);
                // the function shouldn't return here
                syscall(0x03);
            }
            spin_unlock(&kthreadd_lock);
        }
        // TODO: sleep here to wait for new_kernel_thread event
        asm_hlt();
    }
}

void k_new_thread(void (*func)(void*), void* data)
{
    spin_lock(&kthreadd_lock);
    kthreadd_new_thd_func = func;
    kthreadd_new_thd_data = data;
    spin_unlock(&kthreadd_lock);
}

void NORETURN init_scheduler()
{
    processes = types::kernel_allocator_pnew(processes);
    ready_thds = types::kernel_allocator_pnew(ready_thds);
    idx_processes = types::kernel_allocator_pnew(idx_processes);
    idx_child_processes = types::kernel_allocator_pnew(idx_child_processes);

    add_to_process_list(process((void*)kernel_threadd_main));
    auto init = findproc(1);

    // we need interrupts enabled for cow mapping so now we disable it
    // in case timer interrupt mess things up
    asm_cli();

    current_process = init;
    current_thread = init->thds.begin().ptr();

    tss.ss0 = KERNEL_DATA_SEGMENT;
    tss.esp0 = (uint32_t)init->k_esp;

    asm_switch_pd(mms_get_pd(&current_process->mms));

    is_scheduler_ready = true;

    interrupt_stack intrpt_stack {};
    process_context_load(&intrpt_stack, current_process);
    thread_context_load(&intrpt_stack, current_thread);
    to_kernel(&intrpt_stack);
}

void thread_context_save(interrupt_stack* int_stack, thread* thd)
{
    thd->eflags = int_stack->eflags;
    thd->eip = int_stack->v_eip;
    memcpy(&thd->regs, &int_stack->s_regs, sizeof(regs_32));
    if (thd->attr.system)
        thd->regs.esp = int_stack->s_regs.esp + 0x0c;
    else
        thd->regs.esp = int_stack->esp;
}

void thread_context_load(interrupt_stack* int_stack, thread* thd)
{
    int_stack->eflags = (thd->eflags | 0x200); // OR $STI
    int_stack->v_eip = thd->eip;
    memcpy(&int_stack->s_regs, &thd->regs, sizeof(regs_32));
    current_thread = thd;
}

void process_context_save(interrupt_stack*, process*)
{
}

void process_context_load(interrupt_stack*, process* proc)
{
    if (!proc->attr.system)
        tss.esp0 = (uint32_t)proc->k_esp;
    asm_switch_pd(mms_get_pd(&proc->mms));
    current_process = proc;
}

void add_to_process_list(process&& proc)
{
    auto iter = processes->emplace_back(types::move(proc));
    idx_processes->insert(iter->pid, iter);

    auto children = idx_child_processes->find(iter->ppid);
    if (!children) {
        idx_child_processes->insert(iter->ppid, {});
        children = idx_child_processes->find(iter->ppid);
    }

    children->value.push_back(iter->pid);
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

void do_scheduling(interrupt_stack* intrpt_data)
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
        if (current_process)
            process_context_save(intrpt_data, current_process);
        process_context_load(intrpt_data, proc);
    }

    if (current_thread)
        thread_context_save(intrpt_data, current_thread);
    thread_context_load(intrpt_data, thd);

    next_task(iter_thd);

    if (thd->attr.system)
        to_kernel(intrpt_data);
    else
        to_user(intrpt_data);
}
