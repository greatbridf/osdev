#pragma once
#include <errno.h>
#include <kernel/interrupt.h>
#include <kernel/process.hpp>
#include <kernel/vfs.hpp>
#include <stdint.h>
#include <types/size.h>
#include <types/status.h>

namespace types::elf {
using elf32_addr_t = uint32_t;
using elf32_off_t = uint32_t;

using elf_addr_t = elf32_addr_t;
using elf_off_t = elf32_off_t;

constexpr elf32_addr_t ELF_STACK_BOTTOM = 0xbffff000;
constexpr elf32_off_t ELF_STACK_SIZE = 8 * 1024 * 1024;
constexpr elf32_addr_t ELF_STACK_TOP = ELF_STACK_BOTTOM - ELF_STACK_SIZE;

struct PACKED elf32_header {
    // 0x7f, "ELF"
    char magic[4];

    enum : uint8_t {
        FORMAT_32 = 1,
        FORMAT_64 = 2,
    } format;
    enum : uint8_t {
        ENDIAN_LITTLE = 1,
        ENDIAN_BIG = 2,
    } endian;
    // should be 1
    uint8_t _version1;
    enum : uint8_t {
        ABI_SYSTEM_V = 0x00,
        // TODO:
        ABI_LINUX = 0x03,
    } abi;
    uint8_t abi_version;
    uint8_t _reserved[7];
    enum : uint16_t {
        ET_NONE = 0x00,
        ET_REL = 0x01,
        ET_EXEC = 0x02,
        ET_DYN = 0x03,
        ET_CORE = 0x04,
        ET_LOOS = 0xfe00,
        ET_HIOS = 0xfeff,
        ET_LOPROC = 0xff00,
        ET_HIPROC = 0xffff,
    } type;
    enum : uint16_t {
        ARCH_NONE = 0x00,
        ARCH_X86 = 0x03,
        ARCH_ARM = 0x28,
        ARCH_IA64 = 0x32,
        ARCH_X86_64 = 0x3e,
        ARCH_ARM64 = 0xb7,
        ARCH_RISCV = 0xf3,
    } arch;
    // should be 1
    uint32_t _version2;
    // entry address
    elf32_addr_t entry;
    // program header table offset
    elf32_off_t phoff;
    // section header table offset
    elf32_off_t shoff;
    // architecture dependent flags
    uint32_t flags;
    // elf header size
    uint16_t ehsize;
    // program header table entry size
    uint16_t phentsize;
    // program header table entries number
    uint16_t phnum;
    // section header table entry size
    uint16_t shentsize;
    // section header table entries number
    uint16_t shnum;
    // section header table entry index that contains section names
    uint16_t shstrndx;
};

struct PACKED elf32_program_header_entry {
    enum : uint32_t {
        PT_NULL = 0x00,
        PT_LOAD = 0x01,
        PT_DYNAMIC = 0x02,
        PT_INTERP = 0x03,
        PT_NOTE = 0x04,
        PT_SHLIB = 0x05,
        PT_PHDR = 0x06,
        PT_TLS = 0x07,
        PT_LOOS = 0x60000000,
        PT_HIOS = 0x6fffffff,
        PT_LIPROC = 0x70000000,
        PT_HIPROC = 0x7fffffff,
    } type;
    elf32_off_t offset;
    elf32_addr_t vaddr;
    elf32_addr_t paddr;
    elf32_off_t filesz;
    elf32_off_t memsz;
    // segment dependent
    uint32_t flags;
    // 0 and 1 for no alignment, otherwise power of 2
    uint32_t align;
};

struct PACKED elf32_section_header_entry {
    elf32_off_t sh_name;
    enum : uint32_t {
        SHT_NULL = 0x00,
        SHT_PROGBITS = 0x01,
        SHT_RELA = 0x04,
        SHT_DYNAMIC = 0x06,
        SHT_NOTE = 0x07,
        SHT_NOBITS = 0x08,
        SHT_REL = 0x09,
        SHT_DYNSYM = 0x0b,
        SHT_INIT_ARRAY = 0x0e,
        SHT_FINI_ARRAY = 0x0f,
        SHT_PREINIT_ARRAY = 0x0f,
    } sh_type;
    enum : uint32_t {
        SHF_WRITE = 0x01,
        SHF_ALLOC = 0x02,
        SHF_EXECINSTR = 0x04,
    } sh_flags;
    elf32_addr_t sh_addr;
    elf32_off_t sh_offset;
    uint32_t sh_size;
    char _[16];
};

struct elf32_load_data {
    const fs::vfs::dentry* exec_dent;
    const char* const* argv;
    const char* const* envp;
    int errcode;
    void* eip;
    uint32_t* sp;
    bool system;
};

// TODO: environment variables
int elf32_load(elf32_load_data* data);

} // namespace types::elf
