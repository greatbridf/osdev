#include <asm/port_io.h>
#include <fs/fat.hpp>
#include <kernel/hw/ata.hpp>
#include <kernel/stdio.hpp>
#include <kernel/syscall.hpp>
#include <kernel/vfs.hpp>
#include <types/allocator.hpp>
#include <types/assert.h>
#include <types/status.h>
#include <types/stdint.h>

hw::ata::ata(port_id_t p)
    : data(p)
    , error(p + 1)
    , feats(p + 1)
    , count(p + 2)
    , lbalo(p + 3)
    , lbami(p + 4)
    , lbahi(p + 5)
    , drive(p + 6)
    , stats(p + 7)
    , comms(p + 7)
    , slave_flag(0x00)
{
}

hw::ata::stat_t hw::ata::status(void) const
{
    return hw::ata::stat_t { *stats };
}

bool hw::ata::identify(void) const
{
    char buf[512] {};

    drive = 0xa0 | slave_flag;
    count = 0;
    lbalo = 0;
    lbami = 0;
    lbahi = 0;
    comms = 0xec;

    stat_t stat {};
    while ((stat = status()).in.bsy)
        ;

    if (stat.in.err)
        return false;

    read_data(buf, 512);

    if (!status().in.rdy)
        return false;

    return true;
}

int hw::ata::select(bool master)
{
    if (master)
        slave_flag = 0x00;
    else
        slave_flag = 0x10;

    drive = 0xe0 | slave_flag;
    return GB_OK;
}

size_t hw::ata::read_data(char* buf, size_t n) const
{
    size_t orig_n = n;
    n /= 2;
    while (status().in.drq && n--) {
        *(uint16_t*)buf = *data;
        buf += sizeof(uint16_t);
    }
    return orig_n - n * 2;
}

size_t hw::ata::write_data(const char* buf, size_t n) const
{
    size_t orig_n = n;
    n /= 2;
    while (status().in.drq && n--) {
        data = *(uint16_t*)buf;
        buf += sizeof(uint16_t);
    }
    return orig_n - n * 2;
}

int hw::ata::read_sector(char* buf, uint32_t lba_low, uint16_t lba_high) const
{
    count = 0x00; // HIGH BYTE
    lbalo = (lba_low >> 24) & 0xff;
    lbami = lba_high & 0xff;
    lbahi = (lba_high >> 8) & 0xff;
    count = 0x01; // LOW BYTE
    lbalo = lba_low & 0xff;
    lbami = (lba_low >> 8) & 0xff;
    lbahi = (lba_low >> 16) & 0xff;
    comms = 0x24; // READ SECTORS EXT

    while (status().in.bsy)
        ;
    if (status().in.drq)
        read_data(buf, 512);
    return GB_OK;
}

int hw::ata::write_sector(const char* buf, uint32_t lba_low, uint16_t lba_high) const
{
    count = 0x00; // HIGH BYTE
    lbalo = (lba_low >> 24) & 0xff;
    lbami = lba_high & 0xff;
    lbahi = (lba_high >> 8) & 0xff;
    count = 0x01; // LOW BYTE
    lbalo = lba_low & 0xff;
    lbami = (lba_low >> 8) & 0xff;
    lbahi = (lba_low >> 16) & 0xff;
    comms = 0x24; // READ SECTORS EXT

    while (status().in.bsy)
        ;
    if (status().in.drq)
        write_data(buf, 512);
    return GB_OK;
}

static hw::ata* ata_pri;
static hw::ata* ata_sec;
constexpr hw::ata** p_ata_pri = &ata_pri;
constexpr hw::ata** p_ata_sec = &ata_sec;

// data1: offset sectors
// data2: limit sectors
template <hw::ata** ata_bus>
size_t _ata_read(fs::special_node* sn, char* buf, size_t buf_size, size_t offset, size_t n)
{
    assert_likely(buf_size >= n);

    char b[512] {};
    char* orig_buf = buf;
    size_t start = sn->data1 + offset / 512;
    size_t end = sn->data1 + (offset + n + 511) / 512;
    if (end > sn->data1 + sn->data2)
        end = sn->data1 + sn->data2;
    offset %= 512;
    for (size_t i = start; i < end; ++i) {
        (void)(*ata_bus)->read_sector(b, i, 0);
        size_t to_copy = 0;
        if (offset)
            to_copy = 512 - offset;
        else
            to_copy = n > 512 ? 512 : n;
        memcpy(buf, b + offset, to_copy);
        offset = 0;
        buf += to_copy;
        n -= to_copy;
    }
    return buf - orig_buf;
}

struct PACKED mbr_part_entry {
    uint8_t attr;
    uint8_t chs_start[3];
    uint8_t type;
    uint8_t chs_end[3];
    uint32_t lba_start;
    uint32_t cnt;
};

struct PACKED mbr {
    uint8_t code[440];
    uint32_t signature;
    uint16_t reserved;
    struct mbr_part_entry parts[4];
    uint16_t magic;
};

static inline void mbr_part_probe(fs::inode* drive, uint16_t major, uint16_t minor)
{
    struct mbr hda_mbr {
    };
    auto* dev = fs::vfs_open("/dev");

    fs::vfs_read(drive, (char*)&hda_mbr, 512, 0, 512);

    for (const auto& part : hda_mbr.parts) {
        if (!part.type)
            continue;

        fs::register_special_block(major, minor++,
            _ata_read<p_ata_pri>,
            nullptr,
            part.lba_start, part.cnt);

        fs::vfs_mknode(dev, "hda1", { .in { .major = 2, .minor = 1 } });
    }
}

// data: void (*func_to_call_next)(void)
void hw::init_ata(void)
{
    ata_pri = types::pnew<types::kernel_allocator>(ata_pri, ATA_PRIMARY_BUS_BASE);
    if (ata_pri->identify())
        ata_pri->select(true);

    ata_sec = types::pnew<types::kernel_allocator>(ata_pri, ATA_SECONDARY_BUS_BASE);
    if (ata_pri->identify())
        ata_pri->select(true);

    // data1: offset sectors
    // data2: limit sectors
    fs::register_special_block(
        2, 0,
        _ata_read<p_ata_pri>,
        nullptr,
        0,
        0xffffffff);

    // data1: offset sectors
    // data2: limit sectors
    fs::register_special_block(
        2, 8,
        _ata_read<p_ata_sec>,
        nullptr,
        0,
        0xffffffff);

    auto* hda = fs::vfs_open("/dev/hda");
    mbr_part_probe(hda->ind, 2, 1);
}
