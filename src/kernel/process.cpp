#include <assert.h>
#include <bits/alltypes.h>
#include <stdint.h>
#include <sys/mount.h>
#include <sys/wait.h>

#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/elf.hpp>
#include <types/types.h>

#include <kernel/async/lock.hpp>
#include <kernel/log.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/process.hpp>
#include <kernel/vfs.hpp>
#include <kernel/vfs/dentry.hpp>

extern "C" void late_init_rust(uintptr_t* out_sp, uintptr_t* out_ip);

void NORETURN _kernel_init(kernel::mem::paging::pfn_t kernel_stack_pfn) {
    kernel::mem::paging::free_pages(kernel_stack_pfn, 9);

    uintptr_t sp, ip;
    late_init_rust(&sp, &ip);

    asm volatile("sti");

    // ------------------------------------------
    // interrupt enabled
    // ------------------------------------------

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
        :
        : "g"(ds), "g"(cs), "g"(sp), "g"(ip)
        : "eax", "memory");

    freeze();
}

void NORETURN init_scheduler(kernel::mem::paging::pfn_t kernel_stack_pfn) {
    procs = new proclist;

    asm volatile(
        "mov %2, %%rdi\n"
        "mov %0, %%rsp\n"
        "sub $24, %%rsp\n"
        "mov %=f, %%rbx\n"
        "mov %%rbx, (%%rsp)\n"   // return address
        "mov %%rbx, 16(%%rsp)\n" // previous frame return address
        "xor %%rbx, %%rbx\n"
        "mov %%rbx, 8(%%rsp)\n" // previous frame rbp
        "mov %%rsp, %%rbp\n"    // current frame rbp

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

void NORETURN freeze(void) {
    for (;;)
        asm volatile("cli\n\thlt");
}
