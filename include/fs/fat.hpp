#pragma once

#include <kernel/mem.h>
#include <kernel/vfs.hpp>
#include <stdint.h>
#include <string.h>
#include <types/size.h>

namespace fs::fat {
using cluster_t = uint32_t;

// for FAT32
struct PACKED old_boot_sector {
    uint8_t jmp_instruction[3];
    char oem_name[8];
    // usually 512
    uint16_t bytes_per_sector;
    uint8_t sectors_per_cluster;
    // 32 for FAT32
    uint16_t reserved_sectors;
    // usually 2
    uint8_t fat_copies;
    // 0 for FAT32
    uint16_t root_directory_entries;
    // valid before FAT32
    uint16_t _sectors_cnt;
    // 0xf8 for hard disk
    uint8_t type;
    // valid before FAT32
    uint16_t _sectors_per_fat;
    // 12
    uint16_t sectors_per_track;
    // 2
    uint16_t heads;
    // 0
    uint16_t hidden_sectors;
};

// for FAT32
struct PACKED ext_boot_sector {
    struct old_boot_sector old;
    // 0
    uint16_t hidden_sector_ext;
    uint32_t sectors_cnt;
    uint32_t sectors_per_fat;
    uint16_t mirror_flags;
    uint16_t fs_version;
    // 2
    cluster_t root_directory;
    // 1
    uint16_t fs_info_sector;
    // usually at 6, 0x0000 or 0xffff if none
    uint16_t backup_boot_sector;
    uint8_t _reserved[12];
    // for int $0x13
    uint8_t drive_number;
    uint8_t _reserved_for_current_head;
    // 0x29
    uint8_t ext_signature;
    uint32_t serial_number;
    char label[11];
    char fs_type[8];
    uint8_t _reserved_blank[420];
    // 0x55, 0xaa
    uint16_t magic;
};

struct PACKED fs_info_sector {
    // 0x41615252
    uint32_t signature_one;
    uint8_t _reserved[480];
    // 0x61417272
    uint32_t signature_two;
    // may be incorrect
    uint32_t free_clusters;
    // hint only
    uint32_t next_free_cluster;
    uint8_t _reserved_two[12];
    // 0xaa550000
    uint32_t sector_signature;
};

struct PACKED directory_entry {
    char filename[8];
    char extension[3];
    struct PACKED {
        uint8_t ro : 1;
        uint8_t hidden : 1;
        uint8_t system : 1;
        uint8_t volume_label : 1;
        uint8_t subdir : 1;
        uint8_t archive : 1;
        uint8_t _reserved : 2;
    } attributes;
    uint8_t _reserved;
    uint8_t c_time_date[5];
    uint16_t access_date;
    uint16_t cluster_hi;
    uint8_t m_time_date[4];
    uint16_t cluster_lo;
    uint32_t size;
};

// TODO: deallocate inodes when dentry is destroyed
class fat32 : public virtual fs::vfs {
private:
    constexpr static uint32_t SECTOR_SIZE = 512;
    constexpr static cluster_t EOC = 0xffffff8;

private:
    uint32_t sector_cnt;
    uint32_t sectors_per_fat;
    uint32_t serial_number;
    uint32_t free_clusters;
    uint32_t next_free_cluster_hint;
    cluster_t root_dir;
    cluster_t data_region_offset;
    // TODO: use block device special node id
    inode* device;
    uint16_t reserved_sectors;
    uint8_t fat_copies;
    uint8_t sectors_per_cluster;
    char label[12];
    cluster_t* fat;

    struct buf_object {
        char* data;
        int ref;
        // bool dirty;
    };
    types::hash_map<cluster_t, buf_object> buf;

    // buf MUST be larger than 512 bytes
    inline void _raw_read_sector(void* buf, uint32_t sector_no);

    // buf MUST be larger than 4096 bytes
    inline void _raw_read_cluster(void* buf, cluster_t no);

    // buffered version, release_cluster(cluster_no) after used
    char* read_cluster(cluster_t no);
    void release_cluster(cluster_t no);

    static constexpr cluster_t cl(const inode* ind)
    {
        return ind->ino;
    }

    static inline cluster_t _rearrange(directory_entry* d)
    {
        return (((cluster_t)d->cluster_hi) << 16) + d->cluster_lo;
    }

    static inline size_t _write_buf_n(char* buf, size_t buf_size, const char* src, size_t n)
    {
        if (n <= buf_size) {
            memcpy(buf, src, n);
            return n;
        } else {
            memcpy(buf, src, buf_size);
            return buf_size;
        }
    }

public:
    fat32(const fat32&) = delete;
    explicit fat32(inode* _device);
    ~fat32();

    virtual size_t inode_read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n) override;
    virtual int inode_stat(dentry* ent, statx* st, unsigned int mask) override;
    virtual int inode_readdir(fs::inode* dir, size_t offset, const fs::vfs::filldir_func& callback) override;
};

}; // namespace fs::fat
