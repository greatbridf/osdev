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
    auto* thd = thds.emplace_back(main_thd).ptr();
    thd->owner = this;

    for (auto& area : val.mms) {
        if (area.is_ident())
            continue;

        mms.mirror_area(area);
    }

    readythds->push(thd);
}

process::process(pid_t _ppid)
    : mms(*kernel_mms)
    , attr { .system = 1 }
    , pid { process::alloc_pid() }
    , ppid { _ppid }
{
    auto thd = thds.emplace_back(this, true);
    readythds->push(thd.ptr());
}

process::process(void (*func)(void), pid_t _ppid)
    : process { _ppid }
{
    auto* esp = &thds.begin().ptr()->esp;

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
        readythds->remove_all(iter.ptr());
}

inline void NORETURN _noreturn_crash(void)
{
    for (;;)
        assert(false);
}

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
    procs->emplace(kernel_threadd_main, 1);

    asm_sti();

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

    _noreturn_crash();
}

void k_new_thread(void (*func)(void*), void* data)
{
    types::lock_guard lck(kthreadd_mtx);
    kthreadd_new_thd_func = func;
    kthreadd_new_thd_data = data;
}

void NORETURN init_scheduler()
{
    procs = types::kernel_allocator_pnew(procs);
    readythds = types::kernel_ident_allocator_pnew(readythds);

    auto* init = procs->emplace(1).ptr();

    // we need interrupts enabled for cow mapping so now we disable it
    // in case timer interrupt mess things up
    asm_cli();

    current_process = init;
    current_thread = init->thds.begin().ptr();

    tss.ss0 = KERNEL_DATA_SEGMENT;
    tss.esp0 = current_thread->kstack;

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

    _noreturn_crash();
}

extern "C" void asm_ctx_switch(uint32_t** curr_esp, uint32_t* next_esp);
void schedule()
{
    auto thd = readythds->query();

    if (current_thread == thd)
        return;

    process* proc = thd->owner;
    if (current_process != proc) {
        asm_switch_pd(proc->mms.m_pd);
        current_process = proc;
    }

    auto* curr_thd = current_thread;

    current_thread = thd;
    tss.esp0 = current_thread->kstack;

    asm_ctx_switch(&curr_thd->esp, thd->esp);
}
