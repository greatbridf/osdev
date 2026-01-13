use posix_types::stat::StatX;

use super::inode::InodeUse;
use crate::kernel::constants::{
    STATX_ATIME, STATX_BLOCKS, STATX_CTIME, STATX_GID, STATX_INO, STATX_MODE, STATX_MTIME,
    STATX_NLINK, STATX_SIZE, STATX_TYPE, STATX_UID,
};
use crate::kernel::vfs::types::Format;
use crate::prelude::KResult;

impl InodeUse {
    pub fn statx(&self, stat: &mut StatX, mask: u32) -> KResult<()> {
        let sb = self.sbget()?;
        let info = self.info.lock();

        if mask & STATX_NLINK != 0 {
            stat.stx_nlink = info.nlink as _;
            stat.stx_mask |= STATX_NLINK;
        }

        if mask & STATX_ATIME != 0 {
            stat.stx_atime = info.atime.into();
            stat.stx_mask |= STATX_ATIME;
        }

        if mask & STATX_MTIME != 0 {
            stat.stx_mtime = info.mtime.into();
            stat.stx_mask |= STATX_MTIME;
        }

        if mask & STATX_CTIME != 0 {
            stat.stx_ctime = info.ctime.into();
            stat.stx_mask |= STATX_CTIME;
        }

        if mask & STATX_SIZE != 0 {
            stat.stx_size = info.size as _;
            stat.stx_mask |= STATX_SIZE;
        }

        stat.stx_mode = 0;
        if mask & STATX_MODE != 0 {
            stat.stx_mode |= info.perm.bits() as u16;
            stat.stx_mask |= STATX_MODE;
        }

        if mask & STATX_TYPE != 0 {
            stat.stx_mode |= self.format.as_raw() as u16;
            if let Format::BLK | Format::CHR = self.format {
                let devid = self.devid()?;
                stat.stx_rdev_major = devid.major as _;
                stat.stx_rdev_minor = devid.minor as _;
            }
            stat.stx_mask |= STATX_TYPE;
        }

        if mask & STATX_INO != 0 {
            stat.stx_ino = self.ino.as_raw();
            stat.stx_mask |= STATX_INO;
        }

        if mask & STATX_BLOCKS != 0 {
            stat.stx_blocks = (info.size + 512 - 1) / 512;
            stat.stx_blksize = sb.info.io_blksize as _;
            stat.stx_mask |= STATX_BLOCKS;
        }

        if mask & STATX_UID != 0 {
            stat.stx_uid = info.uid;
            stat.stx_mask |= STATX_UID;
        }

        if mask & STATX_GID != 0 {
            stat.stx_gid = info.gid;
            stat.stx_mask |= STATX_GID;
        }

        let fsdev = sb.info.device_id;
        stat.stx_dev_major = fsdev.major as _;
        stat.stx_dev_minor = fsdev.minor as _;

        // TODO: support more attributes
        stat.stx_attributes_mask = 0;

        Ok(())
    }
}
