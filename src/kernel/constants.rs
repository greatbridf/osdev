use bitflags::bitflags;

pub const TCGETS: u32 = 0x5401;
pub const TCSETS: u32 = 0x5402;
pub const TIOCGPGRP: u32 = 0x540f;
pub const TIOCSPGRP: u32 = 0x5410;
pub const TIOCGWINSZ: u32 = 0x5413;

pub const PR_SET_NAME: u32 = 15;
pub const PR_GET_NAME: u32 = 16;

pub const SIG_BLOCK: u32 = 0;
pub const SIG_UNBLOCK: u32 = 1;
pub const SIG_SETMASK: u32 = 2;

pub const CLOCK_REALTIME: u32 = 0;
pub const CLOCK_MONOTONIC: u32 = 1;
pub const CLOCK_REALTIME_COARSE: u32 = 5;

pub const EPERM: u32 = 1;
pub const ENOENT: u32 = 2;
pub const ESRCH: u32 = 3;
pub const EINTR: u32 = 4;
pub const EIO: u32 = 5;
pub const ENXIO: u32 = 6;
pub const ENOEXEC: u32 = 8;
pub const EBADF: u32 = 9;
pub const ECHILD: u32 = 10;
pub const EAGAIN: u32 = 11;
pub const ENOMEM: u32 = 12;
pub const EACCES: u32 = 13;
pub const EFAULT: u32 = 14;
pub const EEXIST: u32 = 17;
pub const ENODEV: u32 = 19;
pub const ENOTDIR: u32 = 20;
pub const EISDIR: u32 = 21;
pub const EINVAL: u32 = 22;
pub const ENOTTY: u32 = 25;
pub const ENOSPC: u32 = 28;
pub const ESPIPE: u32 = 29;
// pub const EROFS: u32 = 30;
pub const EPIPE: u32 = 32;
pub const ERANGE: u32 = 34;
pub const ENOSYS: u32 = 38;
pub const ELOOP: u32 = 40;
pub const EOVERFLOW: u32 = 75;

// pub const S_IFIFO: u32 = 0o010000;
pub const S_IFCHR: u32 = 0o020000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFBLK: u32 = 0o060000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFLNK: u32 = 0o120000;
// pub const S_IFSOCK: u32 = 0o140000;
pub const S_IFMT: u32 = 0o170000;

pub const RLIMIT_STACK: u32 = 0x3;

pub const SEEK_SET: u32 = 0;
pub const SEEK_CUR: u32 = 1;
pub const SEEK_END: u32 = 2;

pub const F_DUPFD: u32 = 0;
pub const F_GETFD: u32 = 1;
pub const F_SETFD: u32 = 2;
pub const F_GETFL: u32 = 3;
pub const F_SETFL: u32 = 4;
pub const F_DUPFD_CLOEXEC: u32 = 1030;

pub const STATX_TYPE: u32 = 1;
pub const STATX_MODE: u32 = 2;
pub const STATX_NLINK: u32 = 4;
pub const STATX_UID: u32 = 8;
pub const STATX_GID: u32 = 16;
pub const STATX_ATIME: u32 = 32;
pub const STATX_MTIME: u32 = 64;
pub const STATX_CTIME: u32 = 128;
pub const STATX_INO: u32 = 256;
pub const STATX_SIZE: u32 = 512;
pub const STATX_BLOCKS: u32 = 1024;
// pub const STATX_BASIC_STATS: u32 = 2047;
// pub const STATX_BTIME: u32 = 2048;

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct UserMmapFlags: u32 {
        const MAP_SHARED = 0x01;
        const MAP_PRIVATE = 0x02;
        const MAP_FIXED = 0x10;
        const MAP_ANONYMOUS = 0x20;
    }

    #[derive(Debug, Clone, Copy)]
    pub struct UserMmapProtocol: u32 {
        const PROT_READ = 0x01;
        const PROT_WRITE = 0x02;
        const PROT_EXEC = 0x04;
    }
}
