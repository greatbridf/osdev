#pragma once

#include <types/path.hpp>
#include <types/types.h>

namespace kernel::hw::acpi {

struct PACKED ACPI_table_header {
    char signature[4];
    uint32_t length;
    uint8_t revision;
    uint8_t checksum;
    uint8_t oemid[6];
    uint8_t oem_table_id[8];
    uint32_t oem_revision;
    uint32_t creator_id;
    uint32_t creator_revision;
};

struct PACKED MCFG {
    ACPI_table_header header;
    uint64_t : 64;
    struct MCFG_entry {
        uint64_t base_address;
        uint16_t segment_group;
        uint8_t start_bus;
        uint8_t end_bus;
        uint32_t : 32;
    } entries[];
};

int parse_acpi_tables();
void* get_table(types::string_view name);

} // namespace kernel::hw::acpi
