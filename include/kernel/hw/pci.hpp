#pragma once

#include <functional>
#include <memory>

#include <stdint.h>

#include <types/types.h>

#include <kernel/mem/phys.hpp>

namespace kernel::kinit {

void init_pci();

} // namespace kernel::kinit

namespace kernel::hw::pci {

struct PACKED device_header_base {
    uint16_t vendor;
    uint16_t device;
    uint16_t volatile command;
    uint16_t volatile status;
    uint8_t revision_id;
    uint8_t prog_if;
    uint8_t subclass;
    uint8_t class_code;
    uint8_t cache_line_size;
    uint8_t latency_timer;
    uint8_t header_type;
    uint8_t volatile bist;
};

struct PACKED device_header_type0 {
    device_header_base base;

    uint32_t bars[6];
    uint32_t cardbus_cis_pointer;
    uint16_t subsystem_vendor_id;
    uint16_t subsystem_id;
    uint32_t expansion_rom_base_address;
    uint8_t capabilities_pointer;
    uint8_t __reserved[7];
    uint8_t volatile interrupt_line;
    uint8_t volatile interrupt_pin;
    uint8_t min_grant;
    uint8_t max_latency;
};

struct SegmentGroup {
    mem::physaddr<void, false> base;
    int number;
};

class pci_device {
   private:
    std::shared_ptr<SegmentGroup> segment_group;
    int bus;
    int device;
    int function;
    void* config_space;

    explicit pci_device(std::shared_ptr<SegmentGroup> segment_group, int bus,
                        int device, int function, void* config_space);

   public:
    static pci_device* probe(std::shared_ptr<SegmentGroup> segment_group,
                             int bus, int device, int function);

    device_header_base& header() const;
    device_header_type0& header_type0() const;

    void enableBusMastering();

    template <typename T>
    inline T& at(size_t offset) const {
        return *reinterpret_cast<T*>(reinterpret_cast<uint8_t*>(config_space) +
                                     offset);
    }
};

using driver_t = std::function<int(pci_device&)>;

int register_driver(uint16_t vendor, uint16_t device, driver_t drv);

} // namespace kernel::hw::pci
