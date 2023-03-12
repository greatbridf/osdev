#include <kernel/errno.h>
#include <stdint.h>
#include <stdio.h>
#include <types/assert.h>
#include <types/elf.hpp>

template <typename T>
constexpr void _user_push(uint32_t** sp, T d)
{
    *sp -= sizeof(T);
    *(T*)*sp = d;
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
        d->errcode = EINVAL;
        return GB_FAILED;
    }

    for (int i = 0; i < hdr.phnum; ++i) {
        if (phents->type != types::elf::elf32_program_header_entry::PT_LOAD)
            continue;

        auto ret = mmap((void*)phents->vaddr, phents->memsz, ent_exec->ind, phents->offset, 1, d->system);
        if (ret != GB_OK) {
            d->errcode = ret;
            return GB_FAILED;
        }

        ++phents;
    }

    // map stack area
    auto ret = mmap((void*)types::elf::ELF_STACK_TOP, types::elf::ELF_STACK_SIZE, fs::vfs_open("/dev/null")->ind, 0, 1, 0);
    assert_likely(ret == GB_OK);

    d->eip = (void*)hdr.entry;
    d->sp = reinterpret_cast<uint32_t*>(types::elf::ELF_STACK_BOTTOM);

    auto* sp = &d->sp;

    types::vector<const char*> arr;
    for (const char** ptr = d->argv; *ptr != nullptr; ++ptr) {
        auto len = strlen(*ptr);
        *sp -= (len + 1);
        *sp = (uint32_t*)((uint32_t)*sp & 0xfffffff0);
        memcpy((char*)*sp, *ptr, len + 1);
        arr.push_back((const char*)*sp);
    }

    *sp -= sizeof(const char*) * arr.size();
    *sp = (uint32_t*)((uint32_t)*sp & 0xfffffff0);
    memcpy((char*)*sp, arr.data(), sizeof(const char*) * arr.size());

    _user_push(sp, 0);
    _user_push(sp, 0);
    _user_push(sp, *sp + 8);
    _user_push(sp, arr.size());

    return GB_OK;
}
