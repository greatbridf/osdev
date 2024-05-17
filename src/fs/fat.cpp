#include <algorithm>

#include <assert.h>
#include <ctype.h>
#include <stdint.h>
#include <stdio.h>

#include <types/allocator.hpp>
#include <types/status.h>

#include <fs/fat.hpp>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/module.hpp>
#include <kernel/vfs.hpp>

#define VFAT_FILENAME_LOWERCASE (0x08)
#define VFAT_EXTENSION_LOWERCASE (0x10)

namespace fs::fat {

// buf MUST be larger than 512 bytes
void fat32::_raw_read_sector(void* buf, uint32_t sector_no)
{
    auto nread = _read_sector_range(
            buf, SECTOR_SIZE,
            sector_no, 1
            );

    assert((size_t)nread == SECTOR_SIZE);
}

// buf MUST be larger than 4096 bytes
void fat32::_raw_read_cluster(void* buf, cluster_t no)
{
    // data cluster start from cluster #2
    no -= 2;

    auto nread = _read_sector_range(
            buf, sectors_per_cluster * SECTOR_SIZE,
            data_region_offset + no * sectors_per_cluster,
            sectors_per_cluster);

    assert((size_t)nread == SECTOR_SIZE * sectors_per_cluster);
}

ssize_t fat32::_read_sector_range(void* _buf, size_t buf_size, uint32_t sector_offset, size_t sector_cnt)
{
    buf_size &= ~(SECTOR_SIZE - 1);

    sector_cnt = std::min(sector_cnt, buf_size / SECTOR_SIZE);

    auto* buf = (char*)_buf;

    auto n = fs::block_device_read(device,
            buf, buf_size,
            sector_offset * SECTOR_SIZE,
            sector_cnt * SECTOR_SIZE
        );

    return n;
}

char* fat32::read_cluster(cluster_t no)
{
    auto iter = buf.find(no);
    if (iter) {
        auto& [ idx, buf ] = *iter;
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

int fat32::readdir(fs::inode* dir, size_t offset, const fs::vfs::filldir_func& filldir)
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

            ino_t ino = _rearrange(d);
            auto* ind = get_inode(ino);
            if (!ind) {
                mode_t mode = 0777;
                if (d->attributes.subdir)
                    mode |= S_IFDIR;
                else
                    mode |= S_IFREG;
                ind = cache_inode(d->size, ino, mode, 0, 0);

                ind->nlink = d->attributes.subdir ? 2 : 1;
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
            auto ret = filldir(fname.c_str(), 0, ind, ind->mode & S_IFMT);

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

fat32::fat32(dev_t _device)
    : device { _device }
    , label { }
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

    // read file allocation table
    fat.resize(SECTOR_SIZE * sectors_per_fat / sizeof(cluster_t));
    _read_sector_range(
            fat.data(), SECTOR_SIZE * sectors_per_fat,
            reserved_sectors, sectors_per_fat);

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
        _root_dir_clusters * sectors_per_cluster * SECTOR_SIZE,
        root_dir, S_IFDIR | 0777, 0, 0);

    n->nlink = 2;

    register_root_node(n);
}

size_t fat32::read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n)
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

int fat32::inode_statx(dentry* ent, statx* st, unsigned int mask)
{
    st->stx_mask = 0;
    if (mask & STATX_SIZE) {
        st->stx_size = ent->ind->size;
        st->stx_mask |= STATX_SIZE;
    }

    if (mask & STATX_BLOCKS) {
        st->stx_blocks = align_up<12>(ent->ind->size) / 512;
        st->stx_blksize = 4096;
        st->stx_mask |= STATX_BLOCKS;
    }

    if (mask & STATX_NLINK) {
        st->stx_nlink = ent->ind->nlink;
        st->stx_mask |= STATX_NLINK;
    }

    st->stx_mode = 0;
    if (mask & STATX_MODE) {
        st->stx_mode |= ent->ind->mode & ~S_IFMT;
        st->stx_mask |= STATX_MODE;
    }

    if (mask & STATX_TYPE) {
        st->stx_mode |= ent->ind->mode & S_IFMT;
        st->stx_mask |= STATX_TYPE;
    }

    if (mask & STATX_INO) {
        st->stx_ino = ent->ind->ino;
        st->stx_mask |= STATX_INO;
    }

    if (mask & STATX_UID) {
        st->stx_uid = ent->ind->uid;
        st->stx_mask |= STATX_UID;
    }

    if (mask & STATX_GID) {
        st->stx_gid = ent->ind->gid;
        st->stx_mask |= STATX_GID;
    }

    return GB_OK;
}

int fat32::inode_stat(dentry* dent, struct stat* st)
{
    auto* ind = dent->ind;

    memset(st, 0x00, sizeof(struct stat));
    st->st_mode = ind->mode;
    st->st_dev = device;
    st->st_nlink = S_ISDIR(ind->mode) ? 2 : 1;
    st->st_size = ind->size;
    st->st_blksize = 4096;
    st->st_blocks = (ind->size + 511) / 512;
    st->st_ino = ind->ino;
    return GB_OK;
}

static fat32* create_fat32(dev_t device)
{
    return new fat32(device);
}

class fat32_module : public virtual kernel::module::module {
public:
    fat32_module() : module("fat32") { }
    ~fat32_module()
    {
        // TODO: unregister filesystem
    }

    virtual int init() override
    {
        int ret = fs::register_fs("fat32", create_fat32);

        if (ret != 0)
            return kernel::module::MODULE_FAILED;

        return kernel::module::MODULE_SUCCESS;
    }
};

} // namespace fs::fat

static kernel::module::module* fat32_module_init()
{ return new fs::fat::fat32_module; }

INTERNAL_MODULE(fat32_module_loader, fat32_module_init);
