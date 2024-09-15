#include <defs.hpp>

#include <assert.h>
#include <errno.h>

#include <driver/e1000e.hpp>
#include <kernel/hw/pci.hpp>
#include <kernel/irq.hpp>
#include <kernel/log.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/mem/phys.hpp>
#include <kernel/module.hpp>
#include <net/arp.hpp>
#include <net/ethernet.hpp>
#include <net/netdev.hpp>

using namespace kernel::kmod;
using namespace kernel::hw::pci;
using namespace kernel::mem;
using namespace net;

using namespace hw::e1000e;

constexpr int E1000E_RX_DESC_COUNT = 32;
constexpr int E1000E_TX_DESC_COUNT = 32;

class e1000eDevice : public virtual Netdev {
    u8* base;

    // rx at 0x0000, tx at 0x0200
    paging::page* rtDescriptorPage;

    int rxHead;
    int rxTail;

    int txHead;
    int txTail;

    void* at(long offset) const;
    void write(long offset, u32 value) const;
    u32 read(long offset) const;

    physaddr<RxDescriptor> rxDescriptors() const;
    physaddr<TxDescriptor> txDescriptors() const;

    virtual int up() override;
    virtual isize send(const u8* buffer, usize length) override;

    int reset();
    int getMacAddress();
    int clearStats();
    int clearRxTxDescriptorTable();

    int setupRx();
    int setupTx();

   public:
    static int probe(pci_device& dev);

    e1000eDevice(pci_device& dev, u8* base);
    virtual ~e1000eDevice();
};

e1000eDevice::e1000eDevice(pci_device& dev, u8* base)
    : Netdev(dev)
    , base(base)
    , rtDescriptorPage(paging::alloc_page())
    , rxHead(-1)
    , rxTail(-1)
    , txHead(-1)
    , txTail(-1) {
    auto pPage = physaddr<u8>{paging::page_to_pfn(rtDescriptorPage)};
    memset(pPage, 0, 0x1000);

    auto pRxDescriptors = this->rxDescriptors();
    for (int i = 0; i < E1000E_RX_DESC_COUNT; i++) {
        auto& desc = pRxDescriptors[i];

        auto bufPage = paging::alloc_pages(2);
        desc.bufferAddress = paging::page_to_pfn(bufPage);
    }

    clearRxTxDescriptorTable();

    getMacAddress();
}

// TODO: make sure all transactions are complete before freeing resources
e1000eDevice::~e1000eDevice() {
    auto pRxDescriptors = this->rxDescriptors();
    for (int i = 0; i < E1000E_RX_DESC_COUNT; i++) {
        auto& desc = pRxDescriptors[i];

        auto* bufPage = paging::pfn_to_page(desc.bufferAddress);
        paging::free_pages(bufPage, 2);

        desc.bufferAddress = 0;
    }

    paging::free_page(rtDescriptorPage);
}

void* e1000eDevice::at(long offset) const {
    return base + offset;
}

void e1000eDevice::write(long offset, u32 value) const {
    *reinterpret_cast<u32 volatile*>(at(offset)) = value;
}

u32 e1000eDevice::read(long offset) const {
    return *reinterpret_cast<u32 volatile*>(at(offset));
}

physaddr<RxDescriptor> e1000eDevice::rxDescriptors() const {
    auto pfn = paging::page_to_pfn(rtDescriptorPage);
    return physaddr<RxDescriptor>(pfn);
}

physaddr<TxDescriptor> e1000eDevice::txDescriptors() const {
    auto pfn = paging::page_to_pfn(rtDescriptorPage);
    return physaddr<TxDescriptor>(pfn + 0x200);
}

int e1000eDevice::reset() {
    // disable interrupts
    write(REG_IMC, 0xffffffff);

    // pcie master disable
    u32 ctrl = read(REG_CTRL);
    ctrl |= CTRL_GIOD;
    write(REG_CTRL, ctrl);

    while (test(read(REG_STATUS), STATUS_GIOE))
        ;

    ctrl = read(REG_CTRL);
    ctrl |= CTRL_RST;
    write(REG_CTRL, ctrl);

    while (test(read(REG_CTRL), CTRL_RST))
        ;

    // disable interrupts again
    write(REG_IMC, 0xffffffff);

    return 0;
}

int e1000eDevice::getMacAddress() {
    memcpy(&mac, at(0x5400), 6);
    return 0;
}

int e1000eDevice::setupRx() {
    auto pRxDescriptors = this->rxDescriptors();

    write(REG_RDBAL, pRxDescriptors.phys() & 0xffffffff);
    write(REG_RDBAH, pRxDescriptors.phys() >> 32);

    write(REG_RDLEN, E1000E_RX_DESC_COUNT * sizeof(RxDescriptor));

    write(REG_RDH, 0);
    rxHead = 0;

    write(REG_RDT, E1000E_RX_DESC_COUNT - 1);
    rxTail = E1000E_RX_DESC_COUNT - 1;

    write(REG_RCTL, RCTL_EN | RCTL_UPE | RCTL_MPE | RCTL_LPE | RCTL_LBM_NO |
                        RCTL_DTYP_LEGACY | RCTL_BAM | RCTL_BSIZE_8192 |
                        RCTL_SECRC);

    return 0;
}

int e1000eDevice::setupTx() {
    auto pTxDescriptors = this->txDescriptors();

    write(REG_TDBAL, pTxDescriptors.phys() & 0xffffffff);
    write(REG_TDBAH, pTxDescriptors.phys() >> 32);

    write(REG_TDLEN, E1000E_TX_DESC_COUNT * sizeof(TxDescriptor));

    write(REG_TDH, 0);
    txHead = 0;

    write(REG_TDT, 0);
    txTail = 0;

    write(REG_TCTL, TCTL_EN | TCTL_PSP | (15 << TCTL_CT_SHIFT) |
                        (64 << TCTL_COLD_SHIFT) | TCTL_RTLC);

    return 0;
}

int e1000eDevice::up() {
    // set link up
    u32 ctrl = read(REG_CTRL);
    u32 stat = read(REG_STATUS);

    if (!test(ctrl, CTRL_SLU) || !test(stat, STATUS_LU))
        return -EIO;

    // auto negotiation speed
    switch (stat & STATUS_SPEED_MASK) {
        case STATUS_SPEED_10M:
            setLinkSpeed(NETDEV_SPEED_10M);
            break;
        case STATUS_SPEED_100M:
            setLinkSpeed(NETDEV_SPEED_100M);
            break;
        case STATUS_SPEED_1000M:
            setLinkSpeed(NETDEV_SPEED_1000M);
            break;
        default:
            return -EINVAL;
    }

    // clear multicast table
    for (int i = 0; i < 128; i += 4)
        write(0x5200 + i, 0);

    clearStats();

    // enable interrupts
    write(REG_IMS, ICR_NORMAL | ICR_UP);

    // read to clear any pending interrupts
    read(REG_ICR);

    kernel::irq::register_handler(
        device.header_type0().interrupt_line, [this]() {
            auto cause = read(REG_ICR);

            if (!test(cause, ICR_INT))
                return;

            rxHead = read(REG_RDH);

            auto nextTail = (rxTail + 1) % E1000E_RX_DESC_COUNT;
            while (nextTail != rxHead) {
                auto pRxDescriptors = rxDescriptors();
                auto& desc = pRxDescriptors[nextTail];

                assert(desc.status & RXD_STAT_DD);

                auto pBuf = physaddr<u8>{desc.bufferAddress};
                kmsg("==== e1000e: received packet ====");

                char buf[256];
                for (int i = 0; i < desc.length; i++) {
                    if (i && i % 16 == 0)
                        kmsg("");

                    snprintf(buf, sizeof(buf), "%x ", pBuf[i]);
                    kernel::tty::console->print(buf);
                }

                kmsg("\n==== e1000e: end of packet ====");

                desc.status = 0;
                rxTail = nextTail;
                nextTail = (nextTail + 1) % E1000E_RX_DESC_COUNT;
            }

            write(REG_RDT, rxTail);
        });

    int ret = setupRx();
    if (ret != 0)
        return ret;

    ret = setupTx();
    if (ret != 0)
        return ret;

    status &= ~NETDEV_DOWN;
    status |= NETDEV_UP;

    return 0;
}

isize e1000eDevice::send(const u8* buffer, usize length) {
    auto txTailNext = (txTail + 1) % E1000E_TX_DESC_COUNT;
    if (txTailNext == txHead)
        return -EAGAIN;

    auto pTxDescriptors = this->txDescriptors();
    auto& desc = pTxDescriptors[txTail];

    if (!(desc.status & TXD_STAT_DD))
        return -EIO;

    auto bufPage = paging::alloc_page();
    auto pPage = physaddr<u8>{paging::page_to_pfn(bufPage)};
    memcpy(pPage, buffer, length);

    desc.bufferAddress = pPage.phys();
    desc.length = length;
    desc.cmd = TXD_CMD_EOP | TXD_CMD_IFCS | TXD_CMD_RS;
    desc.status = 0;

    txTail = txTailNext;
    write(REG_TDT, txTailNext);

    while (!test(desc.status, TXD_STAT_DD))
        ;

    return 0;
}

int e1000eDevice::clearStats() {
    write(REG_COLC, 0);
    write(REG_GPRC, 0);
    write(REG_MPRC, 0);
    write(REG_GPTC, 0);
    write(REG_GORCL, 0);
    write(REG_GORCH, 0);
    write(REG_GOTCL, 0);
    write(REG_GOTCH, 0);

    return 0;
}

int e1000eDevice::clearRxTxDescriptorTable() {
    auto pRxDescriptors = this->rxDescriptors();
    for (int i = 0; i < E1000E_RX_DESC_COUNT; i++) {
        auto& desc = pRxDescriptors[i];
        desc.status = 0;
    }

    auto pTxDescriptors = this->txDescriptors();
    for (int i = 0; i < E1000E_TX_DESC_COUNT; i++) {
        auto& desc = pTxDescriptors[i];
        desc.status = TXD_STAT_DD;
    }

    return 0;
}

int e1000eDevice::probe(pci_device& dev) {
    auto bar0 = dev.header_type0().bars[0];
    if ((bar0 & 0xf) != 0)
        return -EINVAL;

    dev.enableBusMastering();

    auto baseAddress = physaddr<uint8_t, false>{bar0 & ~0xf};
    auto e1000ePointer = std::make_unique<e1000eDevice>(dev, baseAddress);

    int ret = e1000ePointer->reset();
    if (ret != 0)
        return ret;

    ret = e1000ePointer->up();
    if (ret != 0)
        return ret;

    return registerNetdev(std::move(e1000ePointer));
}

class e1000e_module : public virtual kmod {
   public:
    e1000e_module() : kmod("e1000e") {}

    virtual int init() override {
        const int device_id[] = {
            0x100e,
            0x10d3,
            0x10ea,
            0x153a,
        };

        for (auto id : device_id) {
            auto ret = kernel::hw::pci::register_driver(0x8086, id,
                                                        e1000eDevice::probe);

            // TODO: in case any of the devices fail to register,
            // we should return an error code and cleanup these
            // device drivers
            if (ret != 0)
                return ret;
        }

        return 0;
    }
};

INTERNAL_MODULE(e1000e, e1000e_module);
