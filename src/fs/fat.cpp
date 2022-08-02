#include <fs/fat.hpp>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/vfs.hpp>
#include <types/allocator.hpp>
#include <types/assert.h>
#include <types/hash_map.hpp>
#include <types/status.h>
#include <types/stdint.h>

namespace fs::fat {
// buf MUST be larger than 512 bytes
inline void fat32::read_sector(void* buf, uint32_t sector_no)
{
    assert(vfs_read(
               device,
               (char*)buf,
               SECTOR_SIZE,
               sector_no * SECTOR_SIZE,
               SECTOR_SIZE)
        == SECTOR_SIZE);
}

// buf MUST be larger than 4096 bytes
inline void fat32::read_cluster(void* buf, cluster_t no)
{
    // data cluster start from cluster #2
    no -= 2;
    for (int i = 0; i < sectors_per_cluster; ++i) {
        // skip reserved sectors
        read_sector((char*)buf + SECTOR_SIZE * i, data_region_offset + no * sectors_per_cluster + i);
    }
}

int fat32::load_dentry(dentry* ent)
{
    cluster_t next = cl(ent->ind);
    auto buf = (char*)k_malloc(4096);
    do {
        read_cluster(buf, next);
        auto* d = reinterpret_cast<directory_entry*>(buf);
        for (; d->filename[0]; ++d) {
            if (d->attributes.volume_label)
                continue;
            auto* ind = cache_inode({ .in {
                                        .file = !d->attributes.subdir,
                                        .directory = d->attributes.subdir,
                                        .mount_point = 0,
                                        .special_node = 0,
                                    } },
                0777, d->size, (void*)_rearrange(d));
            types::string<> fname;
            for (int i = 0; i < 8; ++i) {
                if (d->filename[i] == ' ')
                    break;
                fname += d->filename[i];
            }
            if (d->extension[0] != ' ') {
                fname += '.';
                fname += d->extension[0];
            }
            for (int i = 1; i < 3; ++i) {
                if (d->extension[i] == ' ')
                    break;
                fname += d->extension[i];
            }
            ent->append(ind, fname);
        }
        next = fat[next];
    } while (next < EOC);
    k_free(buf);
    return GB_OK;
}

fat32::fat32(inode* _device)
    : device(_device)
    , label { 0 }
{
    char* buf = (char*)k_malloc(SECTOR_SIZE);
    read_sector(buf, 0);

    auto* info = reinterpret_cast<ext_boot_sector*>(buf);

    sector_cnt = info->sectors_cnt;
    sectors_per_fat = info->sectors_per_fat;
    sectors_per_cluster = info->old.sectors_per_cluster;
    serial_number = info->serial_number;
    root_dir = info->root_directory;
    reserved_sectors = info->old.reserved_sectors;
    fat_copies = info->old.fat_copies;

    data_region_offset = reserved_sectors + fat_copies * sectors_per_fat;
    fat = (cluster_t*)k_malloc(SECTOR_SIZE * sectors_per_fat);
    // TODO: optimize
    for (uint32_t i = 0; i < 4; ++i)
        read_sector((char*)fat + i * SECTOR_SIZE, reserved_sectors + i);
    for (uint32_t i = 4; i < sectors_per_fat; ++i)
        memset((char*)fat + i * SECTOR_SIZE, 0x00, SECTOR_SIZE);

    int i = 0;
    while (i < 11 && info->label[i] != 0x20) {
        label[i] = info->label[i];
        ++i;
    }
    label[i] = 0x00;

    read_sector(buf, info->fs_info_sector);

    auto* fsinfo = reinterpret_cast<fs_info_sector*>(buf);
    free_clusters = fsinfo->free_clusters;
    next_free_cluster_hint = fsinfo->next_free_cluster;

    k_free(buf);

    size_t _root_dir_clusters = 1;
    cluster_t next = root_dir;
    while ((next = fat[next]) < EOC)
        ++_root_dir_clusters;
    auto* n = cache_inode({ INODE_MNT | INODE_DIR }, 0777, _root_dir_clusters * sectors_per_cluster * SECTOR_SIZE, (void*)root_dir);
    register_root_node(n);
}

fat32::~fat32()
{
    k_free(fat);
}

size_t fat32::inode_read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n)
{
    cluster_t next = reinterpret_cast<cluster_t>(file->impl);
    uint32_t cluster_size = SECTOR_SIZE * sectors_per_cluster;
    auto* b = (char*)k_malloc(cluster_size);
    size_t orig_n = n;

    do {
        if (offset == 0) {
            if (n > cluster_size) {
                read_cluster(buf, next);
                buf_size -= cluster_size;
                buf += cluster_size;
                n -= cluster_size;
            } else {
                read_cluster(b, next);
                auto read = _write_buf_n(buf, buf_size, b, n);
                k_free(b);
                return orig_n - n + read;
            }
        } else {
            if (offset > cluster_size) {
                offset -= cluster_size;
            } else {
                read_cluster(b, next);

                auto to_read = cluster_size - offset;
                if (to_read > n)
                    to_read = n;

                auto read = _write_buf_n(buf, buf_size, b + offset, to_read);
                buf += read;
                n -= read;

                if (read != to_read) {
                    k_free(b);
                    return orig_n - n;
                }

                offset = 0;
            }
        }
        next = fat[next];
    } while (n && next < EOC);

    k_free(b);
    return orig_n - n;
}

int fat32::inode_stat(dentry* ent, stat* st)
{
    st->st_size = ent->ind->size;
    st->st_blksize = 4096;
    st->st_blocks = (ent->ind->size + 4095) / 4096;
    st->st_ino = ent->ind->ino;
    if (ent->ind->flags.in.special_node) {
        st->st_rdev.v = reinterpret_cast<uint32_t>(ent->ind->impl);
    }
    return GB_OK;
}
} // namespace fs::fat
