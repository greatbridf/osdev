#include <asm/port_io.h>
#include <kernel/stdio.h>
#include <kernel/syscall.hpp>
#include <kernel/tty.h>
#include <kernel/hw/ata.hpp>

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
{}

hw::ata::stat_t hw::ata::status(void) const
{
    return hw::ata::stat_t { *stats };
}

void hw::ata::identify(void) const
{
    char buf[512] {};

    drive = 0xa0;
    count = 0;
    lbalo = 0;
    lbami = 0;
    lbahi = 0;
    comms = 0xec;

    stat_t stat {};
    while ((stat = status()).in.bsy)
        ;

    if (stat.in.err)
        syscall(0x03);

    read_data(buf, 512);

    if (!status().in.rdy)
        syscall(0x03);
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

int hw::ata::read_sector(char* buf, uint32_t lba_low, uint16_t lba_high) const
{
    drive = 0x40;                   // SELECT MASTER DRIVE AND LBA48
    count = 0x00;                   // HIGH BYTE
    lbalo = (lba_low >> 24) & 0xff;
    lbami = lba_high & 0xff;
    lbahi = (lba_high >> 8) & 0xff;
    count = 0x01;                   // LOW BYTE
    lbalo = lba_low & 0xff;
    lbami = (lba_low >> 8) & 0xff;
    lbahi = (lba_low >> 16) & 0xff;
    comms = 0x24;                   // READ SECTORS EXT

    while (status().in.bsy)
        ;
    if (status().in.drq)
        read_data(buf, 512);
    return GB_OK;
}

void hw::init_ata(void* data)
{
    if (data != nullptr)
        syscall(0x03);

    ata ata_pri(ATA_PRIMARY_BUS_BASE);
    ata_pri.identify();
    char buf[512] {};
    ata_pri.read_sector(buf, 0, 0);
    tty_print(console, "sector 0 read\n");
    ata_pri.read_sector(buf, 1, 0);
    tty_print(console, "sector 1 read\n");
}
