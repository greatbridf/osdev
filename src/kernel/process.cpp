#include <asm/port_io.h>
#include <asm/sys.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/stdio.h>
#include <kernel_main.h>
#include <types/types.h>
#include <hello-world.res>
#include <interrupt-test.res>

extern "C" void NORETURN go_user_space(void* eip);

static inline void* align_down_to_16byte(void* addr)
{
    return (void*)((uint32_t)addr & 0xfffffff0);
}

static bool is_scheduler_ready;
static types::list<process>* processes;
static types::list<thread*>* ready_thds;

thread* current_thread;
process* current_process;

process::process(process&& val)
    : mms(types::move(val.mms))
    , thds(types::move(val.thds))
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

process::process(void* start_eip, uint8_t* image, size_t image_size, bool system)
    : mms(*kernel_mms)
    , thds {}
    , attr { .system = system }
{
    k_esp = align_down_to_16byte((char*)k_malloc(THREAD_KERNEL_STACK_SIZE) + THREAD_KERNEL_STACK_SIZE);
    memset((char*)k_esp - THREAD_KERNEL_STACK_SIZE, 0x00, THREAD_KERNEL_STACK_SIZE);

    page_directory_entry* pd = alloc_pd();
    memcpy(pd, mms_get_pd(kernel_mms), PAGE_SIZE);
    for (auto& item : mms)
        item.pd = pd;

    auto user_mm = mms.emplace_back(0x40000000U, pd, 1, system);

    auto thd = thds.emplace_back(thread {
        .eip = start_eip,
        .owner = this,
        .regs {},
        .eflags {},
        // TODO: change this
        .esp = 0x40100000U,
    });
    ready_thds->push_back(thd.ptr());

    // TODO: change this
    for (int i = 0; i < 1 * 1024 * 1024 / PAGE_SIZE; ++i)
        k_map(user_mm.ptr(), &empty_page, 1, 1, 0, 1);

    auto* old_pd = reinterpret_cast<page_directory_entry*>(p_ptr_to_v_ptr(current_pd()));
    auto* old_proc = current_process;
    auto* old_thd = current_thread;

    current_process = this;
    current_thread = thd.ptr();
    asm_switch_pd(pd);

    // TODO: change this
    memcpy((void*)0x40000000U, image, image_size);

    current_process = old_proc;
    current_thread = old_thd;
    asm_switch_pd(old_pd);
}

void NORETURN init_scheduler()
{
    processes = types::kernel_allocator_new<types::list<process>>();
    ready_thds = types::kernel_allocator_new<types::list<thread*>>();

    void* user_space_start = reinterpret_cast<void*>(0x40000000U);

    processes->emplace_back(user_space_start, hello_world_bin, hello_world_bin_len, false);
    processes->emplace_back(user_space_start, interrupt_test_bin, interrupt_test_bin_len, false);

    // we need interrupts enabled for cow mapping
    asm_cli();

    auto init_process = processes->begin();
    current_process = init_process.ptr();
    current_thread = init_process->thds.begin().ptr();
    tss.ss0 = KERNEL_DATA_SEGMENT;
    tss.esp0 = (uint32_t)init_process->k_esp;
    asm_switch_pd(mms_get_pd(&current_process->mms));

    is_scheduler_ready = true;
    go_user_space(user_space_start);
}

void thread_context_save(interrupt_stack* int_stack, thread* thd, bool kernel)
{
    thd->eflags = int_stack->eflags;
    thd->eip = int_stack->v_eip;
    memcpy(&thd->regs, &int_stack->s_regs, sizeof(regs_32));
    if (!kernel)
        thd->esp = int_stack->esp;
}

void thread_context_load(interrupt_stack* int_stack, thread* thd, bool kernel)
{
    int_stack->eflags = (thd->eflags | 0x200); // OR $STI
    int_stack->v_eip = thd->eip;
    memcpy(&int_stack->s_regs, &thd->regs, sizeof(regs_32));
    if (!kernel) {
        int_stack->cs = USER_CODE_SELECTOR;
        int_stack->ss = USER_DATA_SELECTOR;
        int_stack->esp = thd->esp;
    } else {
        int_stack->cs = KERNEL_CODE_SEGMENT;
    }
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

void do_scheduling(interrupt_stack* intrpt_data)
{
    if (!is_scheduler_ready)
        return;

    thread* thd = *ready_thds->begin();
    if (current_thread == thd) {
        ready_thds->erase(ready_thds->begin());
        // check if the thread is ready
        ready_thds->push_back(thd);
        return;
    }

    process* proc = thd->owner;
    bool kernel = proc->attr.system;
    if (current_process != proc) {
        process_context_save(intrpt_data, current_process);
        process_context_load(intrpt_data, proc);
    }

    thread_context_save(intrpt_data, current_thread, kernel);
    thread_context_load(intrpt_data, thd, kernel);

    ready_thds->erase(ready_thds->begin());
    // check if the thread is ready
    ready_thds->push_back(thd);

    current_thread = thd;
}
