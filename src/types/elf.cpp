#include <kernel/errno.h>
#include <kernel/mem.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <types/assert.h>
#include <types/elf.hpp>
#include <types/string.hpp>
#include <types/vector.hpp>

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
    auto* ent_exec = fs::vfs_open(d->exec);
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
    auto* phents = (types::elf::elf32_program_header_entry*)k_malloc(phents_size);
    n_read = fs::vfs_read(
        ent_exec->ind,
        (char*)phents,
        phents_size,
        hdr.phoff, phents_size);

    // broken file or I/O error
    if (n_read != phents_size) {
        k_free(phents);

        d->errcode = EINVAL;
        return GB_FAILED;
    }

    // copy argv and envp
    vector<string<>> argv, envp;
    for (const char* const* p = d->argv; *p; ++p)
        argv.emplace_back(*p);
    for (const char* const* p = d->envp; *p; ++p)
        envp.emplace_back(*p);

    // from now on, caller process is recycled.
    // so we can't just simply return to it on error.
    current_process->mms.clear_user();

    for (int i = 0; i < hdr.phnum; ++i) {
        if (phents->type != types::elf::elf32_program_header_entry::PT_LOAD)
            continue;

        auto ret = mmap((void*)phents->vaddr, phents->memsz, ent_exec->ind, phents->offset, 1, d->system);
        if (ret != GB_OK) {
            k_free(phents);

            // TODO: kill process
            assert(false);

            d->errcode = ret;
            return GB_FAILED;
        }

        ++phents;
    }

    // map stack area
    auto ret = mmap((void*)types::elf::ELF_STACK_TOP, types::elf::ELF_STACK_SIZE,
        fs::vfs_open("/dev/null")->ind, 0, 1, 0);
    assert_likely(ret == GB_OK);

    d->eip = (void*)hdr.entry;
    d->sp = reinterpret_cast<uint32_t*>(types::elf::ELF_STACK_BOTTOM);

    auto* sp = (char**)&d->sp;

    // fill information block area
    vector<char*> args, envs;
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
