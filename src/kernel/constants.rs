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

pub const SA_SIGINFO: u32 = 4;

pub const CLOCK_REALTIME: u32 = 0;
pub const CLOCK_MONOTONIC: u32 = 1;

pub const ENXIO: u32 = 6;
pub const ENOEXEC: u32 = 8;

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
