#include <asm/sys.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/stdio.h>
#include <types/types.h>

extern "C" void NORETURN go_user_space(void* eip);

static inline void* align_down_to_16byte(void* addr)
{
    return (void*)((uint32_t)addr & 0xfffffff0);
}

static struct process _init;
process* current_process;

static inline void create_init_process(void)
{
    _init.kernel_esp = align_down_to_16byte(k_malloc(4096 * 1024));
    _init.kernel_ss = 0x10;
    _init.mms = types::kernel_allocator_new<mm_list>(*kernel_mms);

    page_directory_entry* pd = alloc_pd();
    memcpy(pd, mms_get_pd(kernel_mms), PAGE_SIZE);

    for (auto& item : *_init.mms) {
        item.pd = pd;
    }

    _init.mms->push_back(mm {
        .start = 0x40000000,
        .attr = {
            .read = 1,
            .write = 1,
            .system = 0,
        },
        .pgs = types::kernel_allocator_new<page_arr>(),
        .pd = pd,
    });

    auto user_mm = ++_init.mms->begin();

    for (int i = 0; i < 1 * 1024 * 1024 / PAGE_SIZE; ++i) {
        k_map(user_mm.ptr(), &empty_page, 1, 1, 0, 1);
    }

    current_process = &_init;
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

void NORETURN init_scheduler(struct tss32_t* tss)
{
    create_init_process();

    tss->esp0 = (uint32_t)_init.kernel_esp;
    tss->ss0 = _init.kernel_ss;

    go_user_space((void*)0x40000000U);
}
