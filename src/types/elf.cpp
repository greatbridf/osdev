#include <vector>

#include <assert.h>
#include <kernel/errno.h>
#include <kernel/mem.h>
#include <kernel/process.hpp>
#include <kernel/vfs.hpp>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <types/elf.hpp>
#include <types/string.hpp>

#define align16_down(sp) (sp = ((char*)((uint32_t)(sp)&0xfffffff0)))

template <typename T>
inline void _user_push(char** sp, T d)
{
    *sp -= sizeof(T);
    *(T*)*sp = d;
}
template <>
inline void _user_push(char** sp, const char* str)
{
    size_t len = strlen(str);
    *sp -= (len + 1);
    align16_down(*sp);
    memcpy(*sp, str, len + 1);
}

int types::elf::elf32_load(types::elf::elf32_load_data* d)
{
    auto* ent_exec = d->exec_dent;
    if (!ent_exec) {
        d->errcode = ENOENT;
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
        d->errcode = EINVAL;
        return GB_FAILED;
    }

    size_t phents_size = hdr.phentsize * hdr.phnum;
    size_t shents_size = hdr.shentsize * hdr.shnum;
    std::vector<types::elf::elf32_program_header_entry> phents(hdr.phnum);
    n_read = fs::vfs_read(
        ent_exec->ind,
        (char*)phents.data(),
        phents_size,
        hdr.phoff, phents_size);

    // broken file or I/O error
    if (n_read != phents_size) {
        d->errcode = EINVAL;
        return GB_FAILED;
    }

    std::vector<types::elf::elf32_section_header_entry> shents(hdr.shnum);
    n_read = fs::vfs_read(
        ent_exec->ind,
        (char*)shents.data(),
        shents_size,
        hdr.shoff, shents_size);

    // broken file or I/O error
    if (n_read != shents_size) {
        d->errcode = EINVAL;
        return GB_FAILED;
    }

    // copy argv and envp
    std::vector<string<>> argv, envp;
    for (const char* const* p = d->argv; *p; ++p)
        argv.emplace_back(*p);
    for (const char* const* p = d->envp; *p; ++p)
        envp.emplace_back(*p);

    // from now on, caller process is recycled.
    // so we can't just simply return to it on error.
    current_process->mms.clear_user();

    // TODO: remove this
    fs::inode* null_ind = nullptr;
    {
        auto* dent = fs::vfs_open(*fs::fs_root, nullptr, "/dev/null");
        if (!dent)
            kill_current(-1);
        null_ind = dent->ind;
    }

    for (const auto& phent : phents) {
        if (phent.type != types::elf::elf32_program_header_entry::PT_LOAD)
            continue;

        auto vaddr = align_down<12>(phent.vaddr);
        auto vlen = align_up<12>(phent.vaddr + phent.memsz) - vaddr;
        auto flen = align_up<12>(phent.vaddr + phent.filesz) - vaddr;
        auto fileoff = align_down<12>(phent.offset);

        auto ret = mmap(
            (char*)vaddr,
            phent.filesz + (phent.vaddr & 0xfff),
            ent_exec->ind,
            fileoff,
            1,
            d->system);

        if (ret != GB_OK)
            kill_current(-1);

        if (vlen > flen) {
            ret = mmap(
                (char*)vaddr + flen,
                vlen - flen,
                null_ind,
                0,
                1,
                d->system);

            if (ret != GB_OK)
                kill_current(-1);
        }
    }

    for (const auto& shent : shents) {
        if (shent.sh_type == elf32_section_header_entry::SHT_NOBITS)
            memset((char*)shent.sh_addr, 0x00, shent.sh_size);
    }

    // map stack area
    auto ret = mmap((void*)types::elf::ELF_STACK_TOP,
        types::elf::ELF_STACK_SIZE,
        null_ind, 0, 1, 0);
    assert(ret == GB_OK);

    d->eip = (void*)hdr.entry;
    d->sp = reinterpret_cast<uint32_t*>(types::elf::ELF_STACK_BOTTOM);

    auto* sp = (char**)&d->sp;

    // fill information block area
    std::vector<char*> args, envs;
    for (const auto& env : envp) {
        _user_push(sp, env.c_str());
        envs.push_back(*sp);
    }
    for (const auto& arg : argv) {
        _user_push(sp, arg.c_str());
        args.push_back(*sp);
    }

    // push null auxiliary vector entry
    _user_push(sp, 0);
    _user_push(sp, 0);

    // push 0 for envp
    _user_push(sp, 0);

    // push envp
    *sp -= sizeof(void*) * envs.size();
    memcpy(*sp, envs.data(), sizeof(void*) * envs.size());

    // push 0 for argv
    _user_push(sp, 0);

    // push argv
    *sp -= sizeof(void*) * args.size();
    memcpy(*sp, args.data(), sizeof(void*) * args.size());

    // push argc
    _user_push(sp, args.size());

    return GB_OK;
}
