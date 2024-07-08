#include <string>
#include <vector>

#include <assert.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <types/elf.hpp>

#include <kernel/mem/mm_list.hpp>
#include <kernel/mem/vm_area.hpp>
#include <kernel/process.hpp>
#include <kernel/vfs.hpp>

static inline void __user_push32(uintptr_t* sp, uint32_t d)
{
    // TODO: use copy_to_user
    *(--*(uint32_t**)sp) = d;
}

static inline void __user_push_string32(uintptr_t* sp, const char* str)
{
    size_t len = strlen(str);

    *sp -= (len + 1);
    *sp &= ~0xf; // align to 16 bytes

    memcpy((void*)*sp, str, len + 1);
}

int types::elf::elf32_load(types::elf::elf32_load_data& d)
{
    auto& exec = d.exec_dent;
    if (!exec)
        return -ENOENT;

    types::elf::elf32_header hdr {};
    auto n_read = fs::vfs_read(
        exec->ind,
        (char*)&hdr,
        sizeof(types::elf::elf32_header),
        0, sizeof(types::elf::elf32_header));

    if (n_read != sizeof(types::elf::elf32_header))
        return -EINVAL;

    if (hdr.magic[0] != 0x7f || hdr.magic[1] != 'E'
            || hdr.magic[2] != 'L' || hdr.magic[3] != 'F')
        return -EINVAL;

    size_t phents_size = hdr.phentsize * hdr.phnum;
    size_t shents_size = hdr.shentsize * hdr.shnum;
    std::vector<types::elf::elf32_program_header_entry> phents(hdr.phnum);
    n_read = fs::vfs_read(
        exec->ind,
        (char*)phents.data(),
        phents_size,
        hdr.phoff, phents_size);

    // broken file or I/O error
    if (n_read != phents_size)
        return -EINVAL;

    std::vector<types::elf::elf32_section_header_entry> shents(hdr.shnum);
    n_read = fs::vfs_read(
        exec->ind,
        (char*)shents.data(),
        shents_size,
        hdr.shoff, shents_size);

    // broken file or I/O error
    if (n_read != shents_size)
        return -EINVAL;

    // from now on, caller process is gone.
    // so we can't just simply return to it on error.
    auto& mms = current_process->mms;
    mms.clear();

    uintptr_t data_segment_end = 0;

    for (const auto& phent : phents) {
        if (phent.type != types::elf::elf32_program_header_entry::PT_LOAD)
            continue;

        auto vaddr = phent.vaddr & ~0xfff;
        auto vlen = ((phent.vaddr + phent.memsz + 0xfff) & ~0xfff) - vaddr;
        auto flen = ((phent.vaddr + phent.filesz + 0xfff) & ~0xfff) - vaddr;
        auto fileoff = phent.offset & ~0xfff;

        using namespace kernel::mem;
        if (flen) {
            mm_list::map_args args{};

            args.vaddr = vaddr;
            args.length = flen;
            args.file_inode = exec->ind;
            args.file_offset = fileoff;

            args.flags = MM_MAPPED;
            if (phent.flags & elf32_program_header_entry::PF_W)
                args.flags |= MM_WRITE;

            if (phent.flags & elf32_program_header_entry::PF_X)
                args.flags |= MM_EXECUTE;

            if (auto ret = mms.mmap(args); ret != 0)
                return ELF_LOAD_FAIL_NORETURN;
        }

        if (vlen > flen) {
            mm_list::map_args args{};

            args.vaddr = vaddr + flen;
            args.length = vlen - flen;

            args.flags = MM_ANONYMOUS;
            if (phent.flags & elf32_program_header_entry::PF_W)
                args.flags |= MM_WRITE;

            if (phent.flags & elf32_program_header_entry::PF_X)
                args.flags |= MM_EXECUTE;

            if (auto ret = mms.mmap(args); ret != 0)
                return ELF_LOAD_FAIL_NORETURN;
        }

        if (vaddr + vlen > data_segment_end)
            data_segment_end = vaddr + vlen;
    }

    current_process->mms.register_brk(data_segment_end + 0x10000);

    for (const auto& shent : shents) {
        if (shent.sh_type == elf32_section_header_entry::SHT_NOBITS)
            memset((char*)(uintptr_t)shent.sh_addr, 0x00, shent.sh_size);
    }

    // map stack area
    if (1) {
        using namespace kernel::mem;
        mm_list::map_args args{};

        args.vaddr = ELF32_STACK_TOP;
        args.length = ELF32_STACK_SIZE;
        args.flags = MM_ANONYMOUS | MM_WRITE;

        if (auto ret = mms.mmap(args); ret != 0)
            return ELF_LOAD_FAIL_NORETURN;
    }

    d.ip = hdr.entry;
    d.sp = ELF32_STACK_BOTTOM;

    auto* sp = &d.sp;

    // fill information block area
    std::vector<elf32_addr_t> args, envs;
    for (const auto& env : d.envp) {
        __user_push_string32(sp, env.c_str());
        envs.push_back((uintptr_t)*sp);
    }
    for (const auto& arg : d.argv) {
        __user_push_string32(sp, arg.c_str());
        args.push_back((uintptr_t)*sp);
    }

    // push null auxiliary vector entry
    __user_push32(sp, 0);
    __user_push32(sp, 0);

    // push 0 for envp
    __user_push32(sp, 0);

    // push envp
    for (auto ent : envs)
        __user_push32(sp, ent);

    // push 0 for argv
    __user_push32(sp, 0);

    // push argv
    for (int i = args.size()-1; i >= 0; --i)
        __user_push32(sp, args[i]);

    // push argc
    __user_push32(sp, args.size());

    // rename current thread
    current_thread->name = exec->name;

    return 0;
}
