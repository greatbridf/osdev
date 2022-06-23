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

    for (auto& item : thds) {
        item.owner = this;
    }

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

    auto user_mm = mms.emplace_back(mm {
        // TODO: change this
        .start = 0x40000000U,
        .attr = {
            .read = 1,
            .write = 1,
            .system = system,
        },
        .pgs = types::kernel_allocator_new<page_arr>(),
        .pd = pd,
    });

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

    // movl $0x01919810, %eax
    // movl $0x00114514, %ebx
    // jmp $.
    unsigned char instruction1[] = {
        0xb8, 0x10, 0x98, 0x91, 0x01, 0xbb, 0x14, 0x45, 0x11, 0x00, 0xeb, 0xfe
    };

    uint8_t instruction2[] = {
        0xb8, 0x00, 0x81, 0x19, 0x19, 0xbb, 0x00, 0x14, 0x45, 0x11, 0xeb, 0xfe
    };

    void* user_space_start = reinterpret_cast<void*>(0x40000000U);

    processes->emplace_back(user_space_start, instruction1, sizeof(instruction1), false);
    processes->emplace_back(user_space_start, instruction2, sizeof(instruction2), false);

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
            tss.esp0 = (uint32_t)pro->k_esp;
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
