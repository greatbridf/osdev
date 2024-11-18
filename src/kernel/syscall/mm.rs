use bindings::{EINVAL, ENOMEM};

use crate::{
    kernel::{
        constants::{UserMmapFlags, UserMmapProtocol},
        mem::{Mapping, Permission, VAddr},
        task::Thread,
    },
    prelude::*,
};

use super::{define_syscall32, register_syscall, MapArgument, MapArgumentImpl};

/// Check whether we are doing an implemented function.
/// If `condition` is false, return `Err(err)`.
fn check_impl(condition: bool, err: u32) -> KResult<()> {
    if !condition {
        Err(err)
    } else {
        Ok(())
    }
}

fn do_mmap_pgoff(
    addr: usize,
    len: usize,
    prot: UserMmapProtocol,
    flags: UserMmapFlags,
    fd: u32,
    pgoffset: usize,
) -> KResult<usize> {
    let addr = VAddr(addr);
    if addr.floor() != addr || len == 0 {
        return Err(EINVAL);
    }

    let len = (len + 0xfff) & !0xfff;
    check_impl(flags.contains(UserMmapFlags::MAP_ANONYMOUS), ENOMEM)?;
    check_impl(flags.contains(UserMmapFlags::MAP_PRIVATE), EINVAL)?;
    if fd != u32::MAX || pgoffset != 0 {
        return Err(EINVAL);
    }

    let mm_list = &Thread::current().process.mm_list;

    // PROT_NONE, we do unmapping.
    if prot.is_empty() {
        mm_list.unmap(addr, len).map(|_| 0)?;
        return Ok(0);
    }
    // Otherwise, do mmapping.

    // TODO!!!: If we are doing mmap's in 32-bit mode, we should check whether
    //          `addr` is above user reachable memory.
    mm_list
        .mmap(
            addr,
            len,
            Mapping::Anonymous,
            Permission {
                write: prot.contains(UserMmapProtocol::PROT_WRITE),
                execute: prot.contains(UserMmapProtocol::PROT_EXEC),
            },
            flags.contains(UserMmapFlags::MAP_FIXED),
        )
        .map(|addr| addr.0)
}

fn do_munmap(addr: usize, len: usize) -> KResult<usize> {
    let addr = VAddr(addr);
    if addr.floor() != addr || len == 0 {
        return Err(EINVAL);
    }

    let len = (len + 0xfff) & !0xfff;
    Thread::current()
        .process
        .mm_list
        .unmap(addr, len)
        .map(|_| 0)
}

fn do_brk(addr: usize) -> KResult<usize> {
    let vaddr = if addr == 0 { None } else { Some(VAddr(addr)) };
    Ok(Thread::current().process.mm_list.set_break(vaddr).0)
}

impl MapArgument<'_, UserMmapProtocol> for MapArgumentImpl {
    fn map_arg(value: u64) -> UserMmapProtocol {
        UserMmapProtocol::from_bits_truncate(value as u32)
    }
}

impl MapArgument<'_, UserMmapFlags> for MapArgumentImpl {
    fn map_arg(value: u64) -> UserMmapFlags {
        UserMmapFlags::from_bits_truncate(value as u32)
    }
}

define_syscall32!(sys_brk, do_brk, addr: usize);
define_syscall32!(sys_munmap, do_munmap, addr: usize, len: usize);
define_syscall32!(sys_mmap_pgoff, do_mmap_pgoff,
    addr: usize, len: usize,
    prot: UserMmapProtocol,
    flags: UserMmapFlags,
    fd: u32, pgoffset: usize);

pub(super) fn register() {
    register_syscall!(0x2d, brk);
    register_syscall!(0x5b, munmap);
    register_syscall!(0xc0, mmap_pgoff);
}
