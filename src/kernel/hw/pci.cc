#include <map>

#include <assert.h>
#include <errno.h>
#include <stdint.h>

#include <types/types.h>

#include <kernel/hw/acpi.hpp>
#include <kernel/hw/pci.hpp>
#include <kernel/mem/phys.hpp>

using device_no = uint32_t;
constexpr device_no make_device(uint32_t vendor, uint32_t device) {
    return (vendor << 16) | device;
}

namespace kernel::hw::pci {

// lower 16 bits are vendor id, higher 16 bits are device id
std::map<device_no, pci_device*> s_pci_devices;
std::map<device_no, driver_t> s_pci_drivers;

pci_device::pci_device(std::shared_ptr<SegmentGroup> segment_group, int bus,
                       int dev, int func, void* config_space)
    : segment_group(segment_group)
    , bus(bus)
    , device(dev)
    , function(func)
    , config_space(config_space) {}

pci_device* pci_device::probe(std::shared_ptr<SegmentGroup> segmentGroup,
                              int bus, int device, int function) {
    auto configSpaceAddress = segmentGroup->base.phys() + (bus << 20) +
                              (device << 15) + (function << 12);

    auto pConfigSpace =
        mem::physaddr<device_header_base, false>{configSpaceAddress};

    if (pConfigSpace->vendor == 0xffff)
        return nullptr;

    return new pci_device(segmentGroup, bus, device, function, pConfigSpace);
}

void pci_device::enableBusMastering() {
    auto& header = this->header();
    header.command |= 0x4;
}

device_header_base& pci_device::header() const {
    return at<device_header_base>(0);
}

device_header_type0& pci_device::header_type0() const {
    return at<device_header_type0>(0);
}

int register_driver(uint16_t vendor, uint16_t device, driver_t drv) {
    device_no dev = make_device(vendor, device);

    auto iter = s_pci_drivers.find(dev);
    if (iter != s_pci_drivers.end())
        return -EEXIST;

    auto [_, inserted] = s_pci_drivers.emplace(dev, drv);
    assert(inserted);

    auto deviter = s_pci_devices.find(dev);

    // TODO: check status or print log
    if (deviter != s_pci_devices.end())
        drv(*deviter->second);

    return 0;
}

int register_driver_r(uint16_t vendor, uint16_t device,
                      int (*drv)(pci_device*)) {
    return register_driver(vendor, device,
                           [=](pci_device& dev) -> int { return drv(&dev); });
}

} // namespace kernel::hw::pci

namespace kernel::kinit {

SECTION(".text.kinit")
void init_pci() {
    using namespace hw::acpi;
    using namespace hw::pci;

    auto* mcfg = (MCFG*)get_table("MCFG");
    assert(mcfg);

    int n_entries =
        (mcfg->header.length - sizeof(MCFG)) / sizeof(MCFG::MCFG_entry);
    for (int i = 0; i < n_entries; ++i) {
        auto& entry = *&mcfg->entries[i];
        auto segment_group = std::make_shared<SegmentGroup>(
            mem::physaddr<void, false>{entry.base_address},
            entry.segment_group);

        for (int bus = entry.start_bus; bus <= entry.end_bus; ++bus) {
            for (int dev = 0; dev < 32; ++dev) {
                for (int func = 0; func < 8; ++func) {
                    auto* pdev =
                        pci_device::probe(segment_group, bus, dev, func);
                    if (!pdev)
                        break;

                    auto& header = pdev->header();

                    auto [iter, inserted] = s_pci_devices.emplace(
                        make_device(header.vendor, header.device), pdev);
                    assert(inserted);
                }
            }
        }
    }
}

} // namespace kernel::kinit
