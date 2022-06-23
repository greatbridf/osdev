#include <asm/port_io.h>
#include <asm/sys.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/stdio.h>
#include <kernel_main.h>
#include <types/types.h>

extern "C" void NORETURN go_user_space(void* eip);

static inline void* align_down_to_16byte(void* addr)
{
    return (void*)((uint32_t)addr & 0xfffffff0);
}

thread* current_thread;
process* current_process;

static types::list<process>* processes;
static types::list<thread*>* ready_thds;

static inline void create_init_process(void)
{
    auto init = processes->emplace_back();

    init->kernel_esp = align_down_to_16byte((char*)k_malloc(THREAD_KERNEL_STACK_SIZE) + THREAD_KERNEL_STACK_SIZE);
    memset((char*)init->kernel_esp - THREAD_KERNEL_STACK_SIZE, 0x00, THREAD_KERNEL_STACK_SIZE);
    init->attr.system = 0;
    init->mms = *kernel_mms;

    tss.esp0 = (uint32_t)init->kernel_esp;

    page_directory_entry* pd = alloc_pd();
    memcpy(pd, mms_get_pd(kernel_mms), PAGE_SIZE);

    for (auto& item : init->mms) {
        item.pd = pd;
    }

    auto user_mm = init->mms.emplace_back(mm {
        .start = 0x40000000,
        .attr = {
            .read = 1,
            .write = 1,
            .system = 0,
        },
        .pgs = types::kernel_allocator_new<page_arr>(),
        .pd = pd,
    });

    auto thd = init->thds.emplace_back(thread {
        .eip = (void*)0x40000000U,
        .owner = init.ptr(),
        .regs {},
        .eflags {},
        .esp = 0x40100000U,
    });
    ready_thds->push_back(thd.ptr());

    for (int i = 0; i < 1 * 1024 * 1024 / PAGE_SIZE; ++i) {
        k_map(user_mm.ptr(), &empty_page, 1, 1, 0, 1);
    }

    current_process = init.ptr();
    current_thread = thd.ptr();
    asm_switch_pd(pd);

    // movl $0x01919810, %eax
    // movl $0x00114514, %ebx
    // jmp $.
    unsigned char instruction[] = {
        0xb8, 0x10, 0x98, 0x91, 0x01, 0xbb, 0x14, 0x45, 0x11, 0x00, 0xeb, 0xfe
    };

    void* user_mem = (void*)0x40000000U;
    memcpy(user_mem, instruction, sizeof(instruction));
}

static inline void create_test_process(void)
{
    auto proc = processes->emplace_back();
    proc->attr.system = 0;
    proc->kernel_esp = align_down_to_16byte((char*)k_malloc(THREAD_KERNEL_STACK_SIZE) + THREAD_KERNEL_STACK_SIZE);
    memset((char*)proc->kernel_esp - THREAD_KERNEL_STACK_SIZE, 0x00, THREAD_KERNEL_STACK_SIZE);

    proc->mms = *kernel_mms;

    page_directory_entry* pd = alloc_pd();
    memcpy(pd, mms_get_pd(kernel_mms), PAGE_SIZE);
    for (auto& item : proc->mms)
        item.pd = pd;

    auto user_mm = proc->mms.emplace_back(mm {
        .start = 0x40000000,
        .attr = {
            .read = 1,
            .write = 1,
            .system = 0,
        },
        .pgs = types::kernel_allocator_new<page_arr>(),
        .pd = pd,
    });

    auto thd = proc->thds.emplace_back(thread {
        .eip = (void*)0x40000000U,
        .owner = proc.ptr(),
        .regs {},
        .eflags {},
        .esp = 0x40100000U,
    });
    ready_thds->push_back(thd.ptr());

    for (int i = 0; i < 1 * 1024 * 1024 / PAGE_SIZE; ++i)
        k_map(user_mm.ptr(), &empty_page, 1, 1, 0, 1);

    page_directory_entry* init_pd = (page_directory_entry*)p_ptr_to_v_ptr(current_pd());

    auto old_proc = current_process;
    auto old_thd = current_thread;

    current_process = proc.ptr();
    current_thread = thd.ptr();
    asm_switch_pd(pd);

    unsigned char instruction[] = {
        0xb8, 0x00, 0x81, 0x19, 0x19, 0xbb, 0x00, 0x14, 0x45, 0x11, 0xeb, 0xfe
    };

    void* user_mem = (void*)0x40000000U;
    memcpy(user_mem, instruction, sizeof(instruction));

    current_process = old_proc;
    current_thread = old_thd;
    asm_switch_pd(init_pd);
}

static bool is_scheduler_ready;

void NORETURN init_scheduler()
{
    processes = types::kernel_allocator_new<types::list<process>>();
    ready_thds = types::kernel_allocator_new<types::list<thread*>>();

    tss.ss0 = KERNEL_DATA_SEGMENT;

    create_init_process();
    create_test_process();

    asm_cli();
    is_scheduler_ready = true;
    go_user_space((void*)0x40000000U);
}

void context_switch(irq0_data* intrpt_data)
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

    process* pro = thd->owner;
    if (current_process != pro) {
        if (!pro->attr.system) {
            tss.esp0 = (uint32_t)pro->kernel_esp;
        }

        current_process = pro;
        asm_switch_pd(pro->mms.begin()->pd);
    }

    // save current thread info
    current_thread->eflags = intrpt_data->eflags;
    current_thread->eip = intrpt_data->v_eip;
    memcpy(&current_thread->regs, &intrpt_data->s_regs, sizeof(regs_32));

    // load ready thread info
    intrpt_data->eflags = thd->eflags;
    intrpt_data->eflags |= 0x200; // sti
    intrpt_data->v_eip = thd->eip;
    memcpy(&intrpt_data->s_regs, &thd->regs, sizeof(regs_32));

    if (!pro->attr.system) {
        // user mode
        current_thread->esp = intrpt_data->esp;

        intrpt_data->cs = USER_CODE_SELECTOR;
        intrpt_data->ss = USER_DATA_SELECTOR;
        intrpt_data->esp = thd->esp;
    } else {
        // supervisor mode
        intrpt_data->cs = KERNEL_CODE_SEGMENT;
    }

    ready_thds->erase(ready_thds->begin());
    // check if the thread is ready
    ready_thds->push_back(thd);

    current_thread = thd;
}
