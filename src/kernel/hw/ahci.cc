#include "kernel/mem/phys.hpp"
#include <vector>
#include <cstddef>
#include <algorithm>

#include <kernel/vfs.hpp>
#include <kernel/log.hpp>
#include <kernel/mm.hpp>
#include <kernel/module.hpp>
#include <kernel/hw/pci.hpp>
#include <kernel/irq.hpp>

#include <stdint.h>
#include <errno.h>

#define SPIN(cond, spin) \
    (spin) = 0; \
    while ((cond) && (spin) < MAX_SPINS) ++(spin); \
    if ((spin) == MAX_SPINS)

using namespace kernel::module;
using namespace kernel::hw::pci;

using kernel::mem::physaddr;

constexpr uint32_t MAX_SPINS = 100000;

constexpr uint16_t VENDOR_INTEL = 0x8086;
constexpr uint16_t DEVICE_AHCI = 0x2922;

constexpr uint32_t PCI_REG_ABAR = 0x09;

constexpr uint32_t ATA_DEV_BSY = 0x08;
constexpr uint32_t ATA_DEV_DRQ = 0x04;

constexpr uint32_t PORT_CMD_ST = 0x00000001;
constexpr uint32_t PORT_CMD_FRE = 0x00000010;
constexpr uint32_t PORT_CMD_FR = 0x00004000;
constexpr uint32_t PORT_CMD_CR = 0x00008000;

namespace ahci {

typedef volatile struct hba_port_t {
    uint64_t command_list_base;
    uint64_t fis_base;

    uint32_t interrupt_status;
    uint32_t interrupt_enable;

    uint32_t command_status;

    uint32_t : 32; // reserved

    uint32_t task_file_data;
    uint32_t signature;

    uint32_t sata_status;
    uint32_t sata_control;
    uint32_t sata_error;
    uint32_t sata_active;

    uint32_t command_issue;
    uint32_t sata_notification;

    uint32_t fis_based_switch_control;

    uint32_t reserved[11];
    uint32_t vendor[4];
} hba_port;

typedef volatile struct hba_ghc_t {
    uint32_t capabilities;
    uint32_t global_host_control;
    uint32_t interrupt_status;
    uint32_t ports_implemented;
    uint32_t version;
    uint32_t command_completion_coalescing_control;
    uint32_t command_completion_coalescing_ports;
    uint32_t enclosure_management_location;
    uint32_t enclosure_management_control;
    uint32_t host_capabilities_extended;
    uint32_t bios_handoff_control_status;
    uint8_t reserved[0xa0 - 0x2c];
    uint8_t vendor[0x100 - 0xa0];
} hba_ghc;

struct command_header {
    uint8_t command_fis_length : 5;
    uint8_t atapi : 1;
    uint8_t write : 1;
    uint8_t prefetchable : 1;

    uint8_t reset : 1;
    uint8_t bist : 1;
    uint8_t volatile clear_busy_upon_ok : 1;
    uint8_t reserved0 : 1;
    uint8_t port_multiplier : 4;

    uint16_t prdt_length;

    uint32_t volatile bytes_transferred;

    uint64_t command_table_base;

    uint32_t reserved1[4];
};

enum fis_type {
    FIS_REG_H2D = 0x27,
    FIS_REG_D2H = 0x34,
    FIS_DMA_ACT = 0x39,
    FIS_DMA_SETUP = 0x41,
    FIS_DATA = 0x46,
    FIS_BIST = 0x58,
    FIS_PIO_SETUP = 0x5f,
    FIS_DEV_BITS = 0xa1,
};

struct fis_reg_h2d {
    uint8_t fis_type;

    uint8_t pm_port : 4;
    uint8_t : 3; // reserved
    uint8_t is_command : 1;

    uint8_t command;
    uint8_t feature;

    uint8_t lba0;
    uint8_t lba1;
    uint8_t lba2;
    uint8_t device;

    uint8_t lba3;
    uint8_t lba4;
    uint8_t lba5;
    uint8_t feature_high;

    uint16_t count;
    uint8_t iso_command_completion;
    uint8_t control_register;

    uint8_t reserved[4];
};

struct fis_reg_d2h {
    uint8_t fis_type;

    uint8_t pm_port : 4;
    uint8_t : 2; // reserved
    uint8_t interrupt : 1;
    uint8_t : 1; // reserved

    uint8_t status;
    uint8_t error;

    uint8_t lba0;
    uint8_t lba1;
    uint8_t lba2;
    uint8_t device;

    uint8_t lba3;
    uint8_t lba4;
    uint8_t lba5;
    uint8_t : 8; // reserved

    uint16_t count;
    uint8_t reserved1[2];

    uint8_t reserved2[4];
};

struct fis_pio_setup {
    uint8_t fis_type;

    uint8_t pm_port : 4;
    uint8_t : 1; // reserved
    uint8_t data_transfer_direction : 1; // device to host if set
    uint8_t interrupt : 1;
    uint8_t : 1; // reserved

    uint8_t status;
    uint8_t error;

    uint8_t lba0;
    uint8_t lba1;
    uint8_t lba2;
    uint8_t device;

    uint8_t lba3;
    uint8_t lba4;
    uint8_t lba5;
    uint8_t : 8; // reserved

    uint16_t count;
    uint8_t reserved1;
    uint8_t new_status;

    uint16_t transfer_count;
    uint8_t reserved2[2];
};

struct received_fis {
    uint8_t fis_dma_setup[32]; // we don't care about it for now

    fis_pio_setup fispio;
    uint8_t padding[12];

    fis_reg_d2h fisreg;
    uint8_t padding2[4];

    uint8_t fissdb[8];

    uint8_t ufis[64];

    uint8_t reserved[96];
};

struct prdt_entry {
    uint64_t data_base;

    uint32_t reserved0;

    uint32_t byte_count : 22;
    uint32_t reserved1 : 9;
    uint32_t interrupt : 1;
};

struct command_table {
    fis_reg_h2d command_fis;

    uint8_t reserved1[44];

    uint8_t atapi_command[16];

    uint8_t reserved2[48];

    prdt_entry prdt[];
};

static int stop_command(hba_port* port)
{
    port->command_status =
        port->command_status & ~(PORT_CMD_ST | PORT_CMD_FRE);

    uint32_t spins = 0;
    SPIN(port->command_status & (PORT_CMD_CR | PORT_CMD_FR), spins)
        return -1;

    return 0;
}

static int start_command(hba_port* port)
{
    uint32_t spins = 0;
    SPIN(port->command_status & PORT_CMD_CR, spins)
        return -1;

    port->command_status = port->command_status | PORT_CMD_FRE;
    port->command_status = port->command_status | PORT_CMD_ST;

    return 0;
}

static inline hba_port* port_ptr(hba_ghc* ghc, int i)
{
    return (hba_port*)((char*)ghc + 0x100 + i * 0x80);
}

template <std::size_t N>
struct quick_queue {
    std::size_t start { };
    std::size_t end { };
    uint8_t arr[N] { };

    quick_queue()
    {
        for (std::size_t i = 0; i < N; ++i)
            arr[i] = i;
    }

    bool empty() { return start == end; }
    void push(uint8_t val) { arr[end++ % N] = val; }
    uint8_t pop() { return arr[start++ % N]; }
};

struct ahci_port {
private:
    // quick_queue<32> qu;
    physaddr<command_header, false> cmd_header;
    hba_port* port;
    received_fis* fis { };
    std::size_t sectors { -1U };

    int send_command(char* buf, uint64_t lba, uint32_t count, uint8_t cmd, bool write)
    {
        // count must be a multiple of 512
        if (count & (512 - 1))
            return -1;

        // TODO: get an availablee command slot
        int n = 0;
        // auto n = qu.pop();

        // for now, we read 3.5KB at most at a time
        // command fis and prdt will take up the lower 128+Bytes
        physaddr<command_table> cmdtable{nullptr}; // TODO: LONG MODE allocate a page

        // construct command header
        memset(cmd_header + n, 0x00, sizeof(command_header));
        cmd_header[n].command_fis_length = 5;
        cmd_header[n].clear_busy_upon_ok = 1;

        cmd_header[n].write = write;
        cmd_header[n].prdt_length = 1;
        cmd_header[n].command_table_base = cmdtable.phys();

        memset(cmdtable, 0x00, sizeof(command_table) + sizeof(prdt_entry));

        // first, set up command fis
        cmdtable->command_fis.fis_type = FIS_REG_H2D;
        cmdtable->command_fis.is_command = 1;
        cmdtable->command_fis.command = cmd;

        cmdtable->command_fis.lba0 = lba & 0xff;
        cmdtable->command_fis.lba1 = (lba >> 8) & 0xff;
        cmdtable->command_fis.lba2 = (lba >> 16) & 0xff;
        cmdtable->command_fis.device = 1 << 6; // lba mode
        cmdtable->command_fis.lba3 = (lba >> 24) & 0xff;
        cmdtable->command_fis.lba4 = (lba >> 32) & 0xff;
        cmdtable->command_fis.lba5 = (lba >> 40) & 0xff;

        cmdtable->command_fis.count = count >> 9;

        // fill in prdt
        auto* pprdt = cmdtable->prdt;
        pprdt->data_base = cmdtable.phys() + 512;
        pprdt->byte_count = count;
        pprdt->interrupt = 1;

        // clear the received fis
        memset(fis, 0x00, sizeof(received_fis));

        // wait until port is not busy
        uint32_t spins = 0;
        SPIN(port->task_file_data & (ATA_DEV_BSY | ATA_DEV_DRQ), spins)
            return -1;

        // TODO: use interrupt
        // issue the command
        port->command_issue = 1 << n;

        SPIN(port->command_issue & (1 << n), spins)
            return -1;

        memcpy(buf, cmdtable.cast_to<char*>() + 512, count);

        // TODO: free cmdtable
        return 0;
    }

    int identify()
    {
        char buf[512];
        int ret = send_command(buf, 0, 512, 0xEC, false);
        if (ret != 0)
            return -1;
        return 0;
    }

public:
    explicit ahci_port(hba_port* port)
        // TODO: LONG MODE
        : cmd_header{nullptr}, port(port) { }

    ~ahci_port()
    {
        if (!cmd_header)
            return;
        // TODO: free cmd_header
    }

    ssize_t read(char* buf, std::size_t buf_size, std::size_t offset, std::size_t cnt)
    {
        cnt = std::min(buf_size, cnt);

        constexpr size_t READ_BUF_SECTORS = 6;

        char b[READ_BUF_SECTORS * 512] {};
        char* orig_buf = buf;
        size_t start = offset / 512;
        size_t end = std::min((offset + cnt + 511) / 512, sectors);

        offset -= start * 512;
        for (size_t i = start; i < end; i += READ_BUF_SECTORS) {
            size_t n_read = std::min(end - i, READ_BUF_SECTORS) * 512;
            int status = send_command(b, i, n_read, 0xC8, false);
            if (status != 0)
                return -EIO;

            size_t to_copy = std::min(cnt, n_read - offset);
            memcpy(buf, b + offset, to_copy);
            offset = 0;
            buf += to_copy;
            cnt -= to_copy;
        }
        return buf - orig_buf;
    }

    int init()
    {
        if (stop_command(port) != 0)
            return -1;

        // TODO: use interrupt
        // this is the PxIE register, setting bits here will make
        //      it generate corresponding interrupts in PxIS
        //
        // port->interrupt_enable = 1;

        port->command_list_base = cmd_header.phys();
        port->fis_base = cmd_header.phys() + 0x400;

        fis = (received_fis*)(cmd_header + 1);

        if (start_command(port) != 0)
            return -1;

        if (identify() != 0)
            return -1;

        return 0;
    }
};

class ahci_module : public virtual kernel::module::module {
private:
    hba_ghc* ghc { };
    pci_device* dev { };
    std::vector<ahci_port*> ports;

public:
    ahci_module() : module("ahci") { }
    ~ahci_module()
    {
        // TODO: release PCI device
        for (auto& item : ports) {
            if (!item)
                continue;

            delete item;
            item = nullptr;
        }
    }

    int probe_disks()
    {
        int ports = this->ghc->ports_implemented;
        for (int n = 0; ports; ports >>= 1, ++n) {
            if (!(ports & 1))
                continue;

            auto* ghc_port = port_ptr(this->ghc, n);
            if ((ghc_port->sata_status & 0x0f) != 0x03)
                continue;

            auto* port = new ahci_port(ghc_port);
            if (port->init() != 0) {
                delete port;
                kmsg("An error occurred while configuring an ahci port\n");
                continue;
            }

            this->ports[n] = port;

            fs::register_block_device(fs::make_device(8, n * 8), {
                [port](char* buf, std::size_t buf_size, std::size_t offset, std::size_t cnt) {
                    return port->read(buf, buf_size, offset, cnt);
                }, nullptr
            });

            fs::partprobe();
        }

        return 0;
    }

    virtual int init() override
    {
        ports.resize(32);

        auto ret = kernel::hw::pci::register_driver(VENDOR_INTEL, DEVICE_AHCI,
            [this](pci_device* dev) -> int {
                this->dev = dev;

                physaddr<hba_ghc, false> pp_base{dev->reg[PCI_REG_ABAR]};
                this->ghc = pp_base;

                this->ghc->global_host_control =
                    this->ghc->global_host_control | 2; // set interrupt enable

                return this->probe_disks();
        });

        if (ret != 0)
            return MODULE_FAILED;
        return MODULE_SUCCESS;
    }
};

} // namespace ahci

kernel::module::module* ahci_module_init()
{ return new ahci::ahci_module(); }
INTERNAL_MODULE(ahci_module_loader, ahci_module_init);
