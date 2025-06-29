use core::time::Duration;

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct StatXTimestamp {
    pub tv_sec: u64,
    pub tv_nsec: u32,
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct StatX {
    pub stx_mask: u32,
    pub stx_blksize: u32,
    pub stx_attributes: u64,
    pub stx_nlink: u32,
    pub stx_uid: u32,
    pub stx_gid: u32,
    pub stx_mode: u16,
    pub __spare0: [u16; 1usize],
    pub stx_ino: u64,
    pub stx_size: u64,
    pub stx_blocks: u64,
    pub stx_attributes_mask: u64,
    pub stx_atime: StatXTimestamp,
    pub stx_btime: StatXTimestamp,
    pub stx_ctime: StatXTimestamp,
    pub stx_mtime: StatXTimestamp,
    pub stx_rdev_major: u32,
    pub stx_rdev_minor: u32,
    pub stx_dev_major: u32,
    pub stx_dev_minor: u32,
    pub stx_mnt_id: u64,
    pub stx_dio_alignment: [u64; 13usize],
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct TimeSpec {
    pub tv_sec: u64,
    pub tv_nsec: u32,
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct TimeVal {
    pub tv_sec: u64,
    pub tv_usec: u32,
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct Stat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    __padding: u64,

    pub st_size: u64,
    pub st_blksize: u32,
    __padding2: u32,

    pub st_blocks: u64,
    pub st_atime: TimeSpec,
    pub st_mtime: TimeSpec,
    pub st_ctime: TimeSpec,
}

impl From<StatX> for Stat {
    fn from(statx: StatX) -> Self {
        Self {
            st_dev: statx.stx_dev_minor as u64 | ((statx.stx_dev_major as u64) << 8),
            st_ino: statx.stx_ino,
            st_mode: statx.stx_mode as u32,
            st_nlink: statx.stx_nlink,
            st_uid: statx.stx_uid,
            st_gid: statx.stx_gid,
            st_rdev: statx.stx_rdev_minor as u64 | ((statx.stx_rdev_major as u64) << 8),
            __padding: 0,

            st_size: statx.stx_size,
            st_blksize: statx.stx_blksize,
            __padding2: 0,

            st_blocks: statx.stx_blocks,
            st_atime: TimeSpec {
                tv_sec: statx.stx_atime.tv_sec,
                tv_nsec: statx.stx_atime.tv_nsec,
            },
            st_mtime: TimeSpec {
                tv_sec: statx.stx_mtime.tv_sec,
                tv_nsec: statx.stx_mtime.tv_nsec,
            },
            st_ctime: TimeSpec {
                tv_sec: statx.stx_ctime.tv_sec,
                tv_nsec: statx.stx_ctime.tv_nsec,
            },
        }
    }
}

impl From<TimeSpec> for Duration {
    fn from(value: TimeSpec) -> Self {
        Self::new(value.tv_sec, value.tv_nsec)
    }
}
