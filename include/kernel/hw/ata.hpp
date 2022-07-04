#pragma once

#include <asm/port_io.h>
#include <kernel/hw/port.hpp>
#include <kernel/syscall.hpp>
#include <types/cplusplus.hpp>

constexpr port_id_t ATA_PRIMARY_BUS_BASE = 0x1f0;
constexpr port_id_t ATA_PRIMARY_BUS_DEV_CONTROL_OR_ALTER_STATUS = 0x1f0;

constexpr port_id_t ATA_SECONDARY_BUS_BASE = 0x170;
constexpr port_id_t ATA_SECONDARY_BUS_DEV_CONTROL_OR_ALTER_STATUS = 0x1f0;

namespace hw {
class ata {
public:
    union stat_t {
        uint8_t v;
        struct {
            uint8_t err : 1;
            uint8_t idx : 1;
            uint8_t corr : 1;
            uint8_t drq : 1;
            uint8_t srv : 1;
            uint8_t df : 1;
            uint8_t rdy : 1;
            uint8_t bsy : 1;
        } in;
    };

private:
    p16 data;
    p16r error;
    p16w feats;
    p8 count;
    p8 lbalo;
    p8 lbami;
    p8 lbahi;
    p8 drive;
    p8r stats;
    p8w comms;

    uint8_t slave_flag;

public:
    ata(port_id_t port_base);

    stat_t status(void) const;

    void identify(void) const;
    int select(bool master);

    size_t read_data(char* buf, size_t n) const;
    size_t write_data(const char* buf, size_t n) const;

    int read_sector(char* buf, uint32_t lba_low, uint16_t lba_high) const;
    int write_sector(const char* buf, uint32_t lba_low, uint16_t lba_high) const;
};

void init_ata(void* data);
} // namespace hw
