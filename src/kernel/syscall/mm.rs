use super::FromSyscallArg;
use crate::{
    kernel::{
        constants::{UserMmapFlags, UserMmapProtocol},
        mem::{Mapping, Permission},
    },
    prelude::*,
};
use bindings::{EINVAL, ENOMEM};
use eonix_mm::address::{Addr as _, AddrOps as _, VAddr};
use eonix_runtime::task::Task;

impl FromSyscallArg for UserMmapProtocol {
    fn from_arg(value: usize) -> UserMmapProtocol {
        UserMmapProtocol::from_bits_truncate(value as u32)
    }
}

impl FromSyscallArg for UserMmapFlags {
    fn from_arg(value: usize) -> UserMmapFlags {
        UserMmapFlags::from_bits_truncate(value as u32)
    }
}

/// Check whether we are doing an implemented function.
/// If `condition` is false, return `Err(err)`.
fn check_impl(condition: bool, err: u32) -> KResult<()> {
    if !condition {
        Err(err)
    } else {
        Ok(())
    }
}

#[eonix_macros::define_syscall(0xc0)]
fn mmap_pgoff(
    addr: usize,
    len: usize,
    prot: UserMmapProtocol,
    flags: UserMmapFlags,
    fd: u32,
    pgoffset: usize,
) -> KResult<usize> {
    let addr = VAddr::from(addr);
    if !addr.is_page_aligned() || len == 0 {
        return Err(EINVAL);
    }

    let len = (len + 0xfff) & !0xfff;
    check_impl(flags.contains(UserMmapFlags::MAP_ANONYMOUS), ENOMEM)?;
    check_impl(flags.contains(UserMmapFlags::MAP_PRIVATE), EINVAL)?;
    if fd != u32::MAX || pgoffset != 0 {
        return Err(EINVAL);
    }

    let mm_list = &thread.process.mm_list;

    // PROT_NONE, we do unmapping.
    if prot.is_empty() {
        Task::block_on(mm_list.unmap(addr, len)).map(|_| 0)?;
        return Ok(0);
    }
    // Otherwise, do mmapping.

    // TODO!!!: If we are doing mmap's in 32-bit mode, we should check whether
    //          `addr` is above user reachable memory.
    let addr = if flags.contains(UserMmapFlags::MAP_FIXED) {
        mm_list.mmap_fixed(
            addr,
            len,
            Mapping::Anonymous,
            Permission {
                write: prot.contains(UserMmapProtocol::PROT_WRITE),
                execute: prot.contains(UserMmapProtocol::PROT_EXEC),
            },
        )
    } else {
        mm_list.mmap_hint(
            addr,
            len,
            Mapping::Anonymous,
            Permission {
                write: prot.contains(UserMmapProtocol::PROT_WRITE),
                execute: prot.contains(UserMmapProtocol::PROT_EXEC),
            },
        )
    };

    addr.map(|addr| addr.addr())
}

#[eonix_macros::define_syscall(0x5b)]
fn munmap(addr: usize, len: usize) -> KResult<usize> {
    let addr = VAddr::from(addr);
    if !addr.is_page_aligned() || len == 0 {
        return Err(EINVAL);
    }

    let len = (len + 0xfff) & !0xfff;
    Task::block_on(thread.process.mm_list.unmap(addr, len)).map(|_| 0)
}

#[eonix_macros::define_syscall(0x2d)]
fn brk(addr: usize) -> KResult<usize> {
    let vaddr = if addr == 0 { None } else { Some(VAddr::from(addr)) };
    Ok(thread.process.mm_list.set_break(vaddr).addr())
}

#[eonix_macros::define_syscall(0xdb)]
fn madvise(_addr: usize, _len: usize, _advice: u32) -> KResult<()> {
    Ok(())
}

pub fn keep_alive() {}
