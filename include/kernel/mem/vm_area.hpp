#pragma once

#include <stdint.h>

#include <kernel/vfs.hpp>
#include <kernel/vfs/dentry.hpp>

namespace kernel::mem {

constexpr unsigned long MM_WRITE = 0x00000000'00000001;
constexpr unsigned long MM_EXECUTE = 0x00000000'00000002;
constexpr unsigned long MM_MAPPED = 0x00000000'00000004;
constexpr unsigned long MM_ANONYMOUS = 0x00000000'00000008;
constexpr unsigned long MM_INTERNAL_MASK = 0xffffffff'00000000;
constexpr unsigned long MM_BREAK = 0x80000000'00000000;

struct vm_area {
    uintptr_t start;
    uintptr_t end;

    unsigned long flags;

    const fs::rust_inode_handle* mapped_file;
    std::size_t file_offset;

    constexpr bool is_avail(uintptr_t ostart, uintptr_t oend) const noexcept {
        return (ostart >= end || oend <= start);
    }

    constexpr bool operator<(const vm_area& rhs) const noexcept {
        return end <= rhs.start;
    }
    constexpr bool operator<(uintptr_t rhs) const noexcept {
        return end <= rhs;
    }
    friend constexpr bool operator<(uintptr_t lhs,
                                    const vm_area& rhs) noexcept {
        return lhs < rhs.start;
    }

    constexpr vm_area(uintptr_t start, unsigned long flags, uintptr_t end,
                      const fs::rust_inode_handle* mapped_file = nullptr,
                      std::size_t offset = 0)
        : start{start}
        , end{end}
        , flags{flags}
        , mapped_file{mapped_file}
        , file_offset{offset} {}

    constexpr vm_area(uintptr_t start, unsigned long flags,
                      const fs::rust_inode_handle* mapped_file = nullptr,
                      std::size_t offset = 0)
        : start{start}
        , end{start}
        , flags{flags}
        , mapped_file{mapped_file}
        , file_offset{offset} {}
};

} // namespace kernel::mem
