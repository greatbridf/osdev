#pragma once

#include <cstddef>

#include <stdint.h>

namespace kernel::mem {

struct gdt_entry {
    uint64_t limit_low : 16;
    uint64_t base_low : 16;
    uint64_t base_mid : 8;
    uint64_t access : 8;
    uint64_t limit_high : 4;
    uint64_t flags : 4;
    uint64_t base_high : 8;
};

struct e820_mem_map_entry {
    uint64_t base;
    uint64_t len;
    uint32_t type;

    // might not be valid
    uint32_t acpi_extension_attr;
};

namespace info {
    inline std::size_t memory_size;
    inline std::size_t e820_entry_count;
    inline std::size_t e820_entry_length;
    inline e820_mem_map_entry e820_entries[(1024 - 16) / 24];

} // namespace info

} // namespace kernel::mem
