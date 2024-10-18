#pragma once

#include <defs.hpp>
#include <functional>

#include <stdint.h>

#include <kernel/hw/pci.hpp>
#include <net/ethernet.hpp>

namespace net {

constexpr unsigned long NETDEV_UP = 0x001;
constexpr unsigned long NETDEV_DOWN = 0x002;

constexpr unsigned long NETDEV_SPEED_MASK = 0x03c;
constexpr unsigned long NETDEV_SPEED_UNKNOWN = 0x004;
constexpr unsigned long NETDEV_SPEED_10M = 0x008;
constexpr unsigned long NETDEV_SPEED_100M = 0x010;
constexpr unsigned long NETDEV_SPEED_1000M = 0x020;

class Netdev {
   protected:
    unsigned long status;
    MACAddress mac;

    Netdev(kernel::hw::pci::pci_device& device);

   public:
    kernel::hw::pci::pci_device& device;

    virtual ~Netdev() = default;

    int setLinkSpeed(unsigned long speedFlag);

    virtual int up() = 0;
    virtual isize send(const u8* data, usize len) = 0;
};

int registerNetdev(std::unique_ptr<Netdev> dev);

} // namespace net
