#include <map>

#include <assert.h>
#include <errno.h>
#include <stdint.h>

#include <types/types.h>

#include <kernel/hw/pci.hpp>
#include <kernel/hw/port.hpp>

using kernel::hw::p32;

constexpr p32 paddr(0xCF8);
constexpr p32 pdata(0xCFC);

using device_no = uint32_t;
constexpr device_no make_device(uint32_t vendor, uint32_t device) {
    return (vendor << 16) | device;
}

namespace kernel::hw::pci {

// lower 16 bits are vendor id, higher 16 bits are device id
std::map<device_no, pci_device>* pci_devices_p;
std::map<device_no, driver_t>* pci_drivers_p;

// getter of the global variable
std::map<device_no, pci_device>& pci_devices() {
    if (!pci_devices_p) [[unlikely]]
        pci_devices_p = new std::map<device_no, pci_device>();
    return *pci_devices_p;
}

// getter of the global variable
std::map<device_no, driver_t>& pci_drivers() {
    if (!pci_drivers_p) [[unlikely]]
        pci_drivers_p = new std::map<device_no, driver_t>();
    return *pci_drivers_p;
}

// class config_reg

uint32_t config_reg::read32(uint32_t offset) const {
    paddr = (addr_base | (offset & 0xFC));
    return *pdata;
}

uint16_t config_reg::read16(uint16_t offset) const {
    return (read32(offset) >> ((offset & 2) << 3)) & 0xFFFF;
}

uint32_t config_reg::operator[](uint32_t n) const {
    return read32(n << 2);
}

// end class config_reg

// class pci_device

pci_device::pci_device(config_reg reg) : reg(reg) {
    uint32_t tmp = reg[0];

    vendor = tmp & 0xFFFF;
    device = tmp >> 16;

    tmp = reg[2];
    revision_id = tmp & 0xFF;
    subclass = (tmp >> 16) & 0xFF;
    class_code = tmp >> 24;

    tmp = reg[3];
    header_type = (tmp >> 16) & 0xFF;
}

// end class pci_device

pci_device* probe_device(uint8_t bus, uint8_t dev, uint8_t func) {
    config_reg reg(bus, dev, func);

    uint32_t tmp = reg[0];
    uint16_t vendor = tmp & 0xFFFF;
    uint16_t device = tmp >> 16;

    if (vendor == 0xFFFF)
        return nullptr;

    auto [iter, inserted] =
        hw::pci::pci_devices().emplace(make_device(vendor, device), reg);
    assert(inserted);

    return &iter->second;
}

int register_driver(uint16_t vendor, uint16_t device, driver_t drv) {
    auto& drivers = pci_drivers();
    device_no dev = make_device(vendor, device);

    auto iter = drivers.find(dev);
    if (iter != drivers.end())
        return -EEXIST;

    auto [_, inserted] = drivers.emplace(dev, drv);
    assert(inserted);

    auto& devices = pci_devices();
    auto deviter = devices.find(dev);

    // TODO: check status or print log
    if (deviter != devices.end())
        drv(&deviter->second);

    return 0;
}

} // namespace kernel::hw::pci

namespace kernel::kinit {

SECTION(".text.kinit")
void init_pci() {
    for (int bus = 0; bus < 256; ++bus) {
        for (int dev = 0; dev < 32; ++dev) {
            for (int func = 0; func < 8; ++func) {
                auto* pcidev = hw::pci::probe_device(bus, dev, func);
                if (!pcidev)
                    break;
                // TODO: call driver if exists
            }
        }
    }
}

} // namespace kernel::kinit
