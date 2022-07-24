#include <kernel/stdio.h>
#include <kernel/syscall.hpp>
#include <types/elf.hpp>
#include <types/stdint.h>

template <typename T>
constexpr void _user_push(uint32_t& sp, T d)
{
    sp -= sizeof(T);
    *(T*)sp = d;
}

int types::elf::elf32_load(const char* exec, const char** argv, interrupt_stack* intrpt_stack, bool system)
{
    auto* ent_exec = fs::vfs_open(exec);
    if (!ent_exec) {
        intrpt_stack->s_regs.eax = ENOENT;
        intrpt_stack->s_regs.edx = 0;
        return GB_FAILED;
    }

    // TODO: detect file format
    types::elf::elf32_header hdr {};
    auto n_read = fs::vfs_read(
        ent_exec->ind,
        (char*)&hdr,
        sizeof(types::elf::elf32_header),
        0, sizeof(types::elf::elf32_header));

    if (n_read != sizeof(types::elf::elf32_header)) {
        intrpt_stack->s_regs.eax = EINVAL;
        intrpt_stack->s_regs.edx = 0;
        return GB_FAILED;
    }

    size_t phents_size = hdr.phentsize * hdr.phnum;
    auto* phents = (types::elf::elf32_program_header_entry*)k_malloc(phents_size);
    n_read = fs::vfs_read(
        ent_exec->ind,
        (char*)phents,
        phents_size,
        hdr.phoff, phents_size);

    // broken file or I/O error
    if (n_read != phents_size) {
        intrpt_stack->s_regs.eax = EINVAL;
        intrpt_stack->s_regs.edx = 0;
        return GB_FAILED;
    }

    for (int i = 0; i < hdr.phnum; ++i) {
        if (phents->type != types::elf::elf32_program_header_entry::PT_LOAD)
            continue;

        auto ret = mmap((void*)phents->vaddr, phents->memsz, ent_exec->ind, phents->offset, 1, system);
        if (ret != GB_OK) {
            intrpt_stack->s_regs.eax = ret;
            intrpt_stack->s_regs.edx = 0;
            return GB_FAILED;
        }

        ++phents;
    }

    // map stack area
    auto ret = mmap((void*)types::elf::ELF_STACK_TOP, types::elf::ELF_STACK_SIZE, fs::vfs_open("/dev/null")->ind, 0, 1, 0);
    if (ret != GB_OK)
        syscall(0x03);

    intrpt_stack->v_eip = (void*)hdr.entry;
    memset((void*)&intrpt_stack->s_regs, 0x00, sizeof(regs_32));

    auto* sp = &intrpt_stack->s_regs.esp;
    *sp = types::elf::ELF_STACK_BOTTOM;

    types::vector<const char*> arr;
    for (const char** ptr = argv; *ptr != nullptr; ++ptr) {
        auto len = strlen(*ptr);
        *sp -= (len + 1);
        *sp = ((*sp >> 4) << 4);
        memcpy((char*)*sp, *ptr, len + 1);
        arr.push_back((const char*)*sp);
    }

    *sp -= sizeof(const char*) * arr.size();
    *sp = ((*sp >> 4) << 4);
    memcpy((char*)*sp, arr.data(), sizeof(const char*) * arr.size());

    _user_push(*sp, 0);
    _user_push(*sp, 0);
    _user_push(*sp, *sp + 8);
    _user_push(*sp, arr.size());

    return GB_OK;
}
