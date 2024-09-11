#pragma once

#include <functional>

#include <stdint.h>

namespace kernel::kinit {

void init_pci();

} // namespace kernel::kinit

namespace kernel::hw::pci {

struct bar_mmio {
    uint32_t always_zero : 1;
    uint32_t type : 2;
    uint32_t prefetchable : 1;
    uint32_t base_address : 28;
};

struct bar_io {
    uint32_t always_one : 1;
    uint32_t reserved : 1;
    uint32_t base_address : 30;
};

union bar {
    bar_mmio mmio;
    bar_io io;
};

struct config_reg {
    uint32_t addr_base;

    explicit constexpr config_reg(uint32_t bus, uint32_t dev, uint32_t func)
        : addr_base(0x80000000U | (bus << 16) | (dev << 11) | (func << 8)) {}

    // offset is in range from 0x00 to 0xff
    uint32_t read32(uint32_t offset) const;
    uint16_t read16(uint16_t offset) const;

    // read n-th 32-bit register
    uint32_t operator[](uint32_t n) const;
};

struct device_header_base {
    uint16_t vendor;
    uint16_t device;
    uint16_t command;
    uint16_t status;
    uint8_t revision_id;
    uint8_t prog_if;
    uint8_t subclass;
    uint8_t class_code;
    uint8_t cache_line_size;
    uint8_t latency_timer;
    uint8_t header_type;
    uint8_t bist;
};

struct device_header_type0 {
    bar bars[6];
    uint32_t cardbus_cis_pointer;
    uint16_t subsystem_vendor_id;
    uint16_t subsystem_id;
    uint32_t expansion_rom_base_address;
    uint8_t capabilities_pointer;
    uint8_t reserved[7];
    uint8_t interrupt_line;
    uint8_t interrupt_pin;
    uint8_t min_grant;
    uint8_t max_latency;
};

class pci_device {
   public:
    config_reg reg;

    uint16_t vendor;
    uint16_t device;

    uint8_t revision_id;
    uint8_t subclass;
    uint8_t class_code;
    uint8_t header_type;

    explicit pci_device(config_reg reg);
};

using driver_t = std::function<int(pci_device*)>;

pci_device* probe_device(uint8_t bus, uint8_t device, uint8_t function);
int register_driver(uint16_t vendor, uint16_t device, driver_t drv);

} // namespace kernel::hw::pci
