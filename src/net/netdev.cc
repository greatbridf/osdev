#include <map>

#include <errno.h>

#include <net/netdev.hpp>

using namespace net;

static int s_nextNetdevId = 0;
static std::map<int, std::unique_ptr<Netdev>> s_netdevs;

Netdev::Netdev(kernel::hw::pci::pci_device& device)
    : status{NETDEV_DOWN | NETDEV_SPEED_UNKNOWN}, mac{}, device{device} {}

int Netdev::setLinkSpeed(unsigned long speedFlag) {
    status = (status & ~NETDEV_SPEED_MASK) | speedFlag;
    return 0;
}

int net::registerNetdev(std::unique_ptr<Netdev> dev) {
    auto [it, inserted] = s_netdevs.try_emplace(s_nextNetdevId, std::move(dev));
    if (!inserted)
        return -EFAULT;

    return s_nextNetdevId++;
}
