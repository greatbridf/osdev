#include <types/elf.hpp>

int types::elf::elf32_load(const char* exec, interrupt_stack* intrpt_stack, bool system)
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

    intrpt_stack->v_eip = (void*)hdr.entry;
    memset((void*)&intrpt_stack->s_regs, 0x00, sizeof(regs_32));
    intrpt_stack->s_regs.esp = types::elf::ELF_STACK_BOTTOM;
    intrpt_stack->s_regs.ebp = types::elf::ELF_STACK_BOTTOM;
    return GB_OK;
}
