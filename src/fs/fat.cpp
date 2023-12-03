#include <assert.h>
#include <ctype.h>
#include <fs/fat.hpp>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/vfs.hpp>
#include <stdint.h>
#include <stdio.h>
#include <types/allocator.hpp>
#include <types/hash_map.hpp>
#include <types/status.h>

#define VFAT_FILENAME_LOWERCASE (0x08)
#define VFAT_EXTENSION_LOWERCASE (0x10)

namespace fs::fat {
// buf MUST be larger than 512 bytes
inline void fat32::_raw_read_sector(void* buf, uint32_t sector_no)
{
    size_t n = vfs_read(
        device,
        (char*)buf,
        SECTOR_SIZE,
        sector_no * SECTOR_SIZE,
        SECTOR_SIZE);
    assert(n == SECTOR_SIZE);
}

// buf MUST be larger than 4096 bytes
inline void fat32::_raw_read_cluster(void* buf, cluster_t no)
{
    // data cluster start from cluster #2
    no -= 2;
    for (int i = 0; i < sectors_per_cluster; ++i) {
        // skip reserved sectors
        _raw_read_sector((char*)buf + SECTOR_SIZE * i, data_region_offset + no * sectors_per_cluster + i);
    }
}

char* fat32::read_cluster(cluster_t no)
{
    auto iter = buf.find(no);
    if (iter) {
        auto [ idx, buf ] = *iter;
        ++buf.ref;
        return buf.data;
    }
    auto* data = new char[sectors_per_cluster * SECTOR_SIZE];
    _raw_read_cluster(data, no);
    buf.emplace(no,
        buf_object {
            data,
            1,
            // false,
        });
    return data;
}

void fat32::release_cluster(cluster_t no)
{
    auto iter = buf.find(no);
    if (iter)
        --iter->second.ref;
}

int fat32::inode_readdir(fs::inode* dir, size_t offset, fs::vfs::filldir_func filldir)
{
    cluster_t next = cl(dir);
    for (size_t i = 0; i < (offset / (sectors_per_cluster * SECTOR_SIZE)); ++i) {
        if (next >= EOC)
            return 0;
        next = fat[next];
    }
    size_t nread = 0;
    do {
        char* buf = read_cluster(next);
        auto* d = reinterpret_cast<directory_entry*>(buf) + (offset % (sectors_per_cluster * SECTOR_SIZE)) / sizeof(directory_entry);
        offset = 0;
        auto* end = d + (sectors_per_cluster * SECTOR_SIZE / sizeof(directory_entry));
        for (; d < end && d->filename[0]; ++d) {
            if (d->attributes.volume_label) {
                nread += sizeof(directory_entry);
                continue;
            }

            fs::ino_t ino = _rearrange(d);
            auto* ind = get_inode(ino);
            if (!ind) {
                ind = cache_inode({ .in {
                                      .file = !d->attributes.subdir,
                                      .directory = d->attributes.subdir,
                                      .mount_point = 0,
                                      .special_node = 0,
                                  } },
                    0777, d->size, ino);
            }

            types::string<> fname;
            for (int i = 0; i < 8; ++i) {
                if (d->filename[i] == ' ')
                    break;
                if (d->_reserved & VFAT_FILENAME_LOWERCASE)
                    fname += tolower(d->filename[i]);
                else
                    fname += toupper(d->filename[i]);
            }
            if (d->extension[0] != ' ')
                fname += '.';
            for (int i = 1; i < 3; ++i) {
                if (d->extension[i] == ' ')
                    break;
                if (d->_reserved & VFAT_EXTENSION_LOWERCASE)
                    fname += tolower(d->extension[i]);
                else
                    fname += toupper(d->extension[i]);
            }
            auto ret = filldir(fname.c_str(), 0, ind->ino,
                (ind->flags.in.directory || ind->flags.in.mount_point) ? DT_DIR : DT_REG);

            if (ret != GB_OK) {
                release_cluster(next);
                return nread;
            }

            nread += sizeof(directory_entry);
        }
        release_cluster(next);
        next = fat[next];
    } while (next < EOC);
    return nread;
}

fat32::fat32(inode* _device)
    : device(_device)
    , label { 0 }
{
    auto* buf = new char[SECTOR_SIZE];
    _raw_read_sector(buf, 0);

    auto* info = reinterpret_cast<ext_boot_sector*>(buf);

    sector_cnt = info->sectors_cnt;
    sectors_per_fat = info->sectors_per_fat;
    sectors_per_cluster = info->old.sectors_per_cluster;
    serial_number = info->serial_number;
    root_dir = info->root_directory;
    reserved_sectors = info->old.reserved_sectors;
    fat_copies = info->old.fat_copies;

    data_region_offset = reserved_sectors + fat_copies * sectors_per_fat;
    fat = (cluster_t*)new char[SECTOR_SIZE * sectors_per_fat];
    // TODO: optimize
    for (uint32_t i = 0; i < 4; ++i)
        _raw_read_sector((char*)fat + i * SECTOR_SIZE, reserved_sectors + i);
    for (uint32_t i = 4; i < sectors_per_fat; ++i)
        memset((char*)fat + i * SECTOR_SIZE, 0x00, SECTOR_SIZE);

    int i = 0;
    while (i < 11 && info->label[i] != 0x20) {
        label[i] = info->label[i];
        ++i;
    }
    label[i] = 0x00;

    _raw_read_sector(buf, info->fs_info_sector);

    auto* fsinfo = reinterpret_cast<fs_info_sector*>(buf);
    free_clusters = fsinfo->free_clusters;
    next_free_cluster_hint = fsinfo->next_free_cluster;

    delete[] buf;

    size_t _root_dir_clusters = 1;
    cluster_t next = root_dir;
    while ((next = fat[next]) < EOC)
        ++_root_dir_clusters;
    auto* n = cache_inode(
        { INODE_MNT | INODE_DIR },
        0777,
        _root_dir_clusters * sectors_per_cluster * SECTOR_SIZE,
        root_dir);
    register_root_node(n);
}

fat32::~fat32()
{
    delete[]((char*)fat);
}

size_t fat32::inode_read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n)
{
    cluster_t next = cl(file);
    uint32_t cluster_size = SECTOR_SIZE * sectors_per_cluster;
    size_t orig_n = n;

    do {
        if (offset == 0) {
            if (n > cluster_size) {
                auto* data = read_cluster(next);
                memcpy(buf, data, cluster_size);
                release_cluster(next);

                buf_size -= cluster_size;
                buf += cluster_size;
                n -= cluster_size;
            } else {
                auto* data = read_cluster(next);
                auto read = _write_buf_n(buf, buf_size, data, n);
                release_cluster(next);

                return orig_n - n + read;
            }
        } else {
            if (offset > cluster_size) {
                offset -= cluster_size;
            } else {
                auto* data = read_cluster(next);

                auto to_read = cluster_size - offset;
                if (to_read > n)
                    to_read = n;

                auto read = _write_buf_n(buf, buf_size, data + offset, to_read);
                buf += read;
                n -= read;

                release_cluster(next);
                if (read != to_read) {
                    return orig_n - n;
                }

                offset = 0;
            }
        }
        next = fat[next];
    } while (n && next < EOC);

    return orig_n - n;
}

int fat32::inode_stat(dentry* ent, stat* st)
{
    st->st_size = ent->ind->size;
    st->st_blksize = 4096;
    st->st_blocks = (ent->ind->size + 4095) / 4096;
    st->st_ino = ent->ind->ino;
    return GB_OK;
}
} // namespace fs::fat
