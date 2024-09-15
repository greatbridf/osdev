#include <map>

#include <assert.h>
#include <errno.h>

#include <types/path.hpp>
#include <types/types.h>

#include <kernel/hw/acpi.hpp>
#include <kernel/mem/phys.hpp>

using namespace kernel::mem;

namespace kernel::hw::acpi {

static std::map<types::string_view, ACPI_table_header*> acpi_tables;

struct PACKED RSDP {
    uint64_t signature;
    uint8_t checksum;
    uint8_t oemid[6];
    uint8_t revision;
    uint32_t rsdt_addr;
};

struct PACKED RSDT {
    ACPI_table_header header;
    uint32_t entries[];
};

static bool checksum(const void* data, size_t len) {
    uint8_t sum = 0;
    for (size_t i = 0; i < len; ++i)
        sum += reinterpret_cast<const uint8_t*>(data)[i];
    return sum == 0;
}

static const RSDP* _find_rsdp() {
    for (uintptr_t addr = 0xe0000; addr < 0x100000; addr += 0x10) {
        physaddr<RSDP> rsdp{addr};

        // "RSD PTR "
        if (rsdp->signature == 0x2052545020445352) {
            if (checksum(rsdp, sizeof(RSDP)))
                return rsdp;
        }
    }

    return nullptr;
}

static const RSDT* _rsdt() {
    static const RSDT* rsdt;
    if (rsdt)
        return rsdt;

    auto* rsdp = _find_rsdp();
    if (!rsdp || rsdp->revision != 0)
        return nullptr;

    const RSDT* cur_rsdt = physaddr<const RSDT>{rsdp->rsdt_addr};

    constexpr auto rsdt_signature = types::string_view{"RSDT", 4};
    auto signature = types::string_view{cur_rsdt->header.signature, 4};
    if (signature != rsdt_signature)
        return nullptr;
    if (!checksum(cur_rsdt, cur_rsdt->header.length))
        return nullptr;

    return rsdt = cur_rsdt;
}

int parse_acpi_tables() {
    auto rsdt = _rsdt();
    if (!rsdt)
        return -ENOENT;

    auto entries =
        (rsdt->header.length - sizeof(ACPI_table_header)) / sizeof(uint32_t);
    for (uint32_t i = 0; i < entries; ++i) {
        physaddr<ACPI_table_header> header{rsdt->entries[i]};
        if (!checksum(header, header->length))
            continue;

        types::string_view table_name{header->signature, 4};
        acpi_tables[table_name] = header;
    }

    return 0;
}

void* get_table(types::string_view name) {
    auto iter = acpi_tables.find(name);
    if (iter == acpi_tables.end())
        return nullptr;
    return iter->second;
}

} // namespace kernel::hw::acpi
