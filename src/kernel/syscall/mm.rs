use align_ext::AlignExt;
use eonix_mm::address::{Addr as _, AddrOps as _, VAddr};
use eonix_mm::paging::PAGE_SIZE;
use posix_types::syscall_no::*;

use super::FromSyscallArg;
use crate::kernel::constants::{UserMmapFlags, UserMmapProtocol, EBADF, EINVAL};
use crate::kernel::mem::{FileMapping, Mapping, Permission};
use crate::kernel::task::Thread;
use crate::kernel::vfs::filearray::FD;
use crate::prelude::*;

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
#[allow(unused)]
fn check_impl(condition: bool, err: u32) -> KResult<()> {
    if !condition {
        Err(err)
    } else {
        Ok(())
    }
}

async fn do_mmap2(
    thread: &Thread,
    addr: usize,
    len: usize,
    prot: UserMmapProtocol,
    flags: UserMmapFlags,
    fd: FD,
    pgoffset: usize,
) -> KResult<usize> {
    let addr = VAddr::from(addr);
    if !addr.is_page_aligned() || pgoffset % PAGE_SIZE != 0 || len == 0 {
        return Err(EINVAL);
    }

    let len = len.align_up(PAGE_SIZE);
    let mm_list = &thread.process.mm_list;
    let is_shared = flags.contains(UserMmapFlags::MAP_SHARED);

    let mapping = if flags.contains(UserMmapFlags::MAP_ANONYMOUS) {
        if pgoffset != 0 {
            return Err(EINVAL);
        }

        if !is_shared {
            Mapping::Anonymous
        } else {
            unimplemented!("mmap MAP_ANONYMOUS | MAP_SHARED");
        }
    } else {
        let file = thread
            .files
            .get(fd)
            .ok_or(EBADF)?
            .get_inode()?
            .ok_or(EBADF)?;

        Mapping::File(FileMapping::new(file.get_page_cache(), pgoffset, len))
    };

    let permission = Permission {
        read: prot.contains(UserMmapProtocol::PROT_READ),
        write: prot.contains(UserMmapProtocol::PROT_WRITE),
        execute: prot.contains(UserMmapProtocol::PROT_EXEC),
    };

    // TODO!!!: If we are doing mmap's in 32-bit mode, we should check whether
    //          `addr` is above user reachable memory.
    let addr = if flags.contains(UserMmapFlags::MAP_FIXED) {
        mm_list.unmap(addr, len).await?;
        mm_list
            .mmap_fixed(addr, len, mapping, permission, is_shared)
            .await
    } else {
        mm_list
            .mmap_hint(addr, len, mapping, permission, is_shared)
            .await
    };

    addr.map(|addr| addr.addr())
}

#[cfg(any(target_arch = "riscv64", target_arch = "loongarch64"))]
#[eonix_macros::define_syscall(SYS_MMAP)]
async fn mmap(
    addr: usize,
    len: usize,
    prot: UserMmapProtocol,
    flags: UserMmapFlags,
    fd: FD,
    offset: usize,
) -> KResult<usize> {
    do_mmap2(thread, addr, len, prot, flags, fd, offset).await
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_MMAP2)]
async fn mmap2(
    addr: usize,
    len: usize,
    prot: UserMmapProtocol,
    flags: UserMmapFlags,
    fd: FD,
    pgoffset: usize,
) -> KResult<usize> {
    do_mmap2(thread, addr, len, prot, flags, fd, pgoffset).await
}

#[eonix_macros::define_syscall(SYS_MUNMAP)]
async fn munmap(addr: usize, len: usize) -> KResult<()> {
    let addr = VAddr::from(addr);
    if !addr.is_page_aligned() || len == 0 {
        return Err(EINVAL);
    }

    let len = len.align_up(PAGE_SIZE);
    thread.process.mm_list.unmap(addr, len).await
}

#[eonix_macros::define_syscall(SYS_BRK)]
async fn brk(addr: usize) -> KResult<usize> {
    let vaddr = if addr == 0 { None } else { Some(VAddr::from(addr)) };
    Ok(thread.process.mm_list.set_break(vaddr).await.addr())
}

#[eonix_macros::define_syscall(SYS_MADVISE)]
async fn madvise(_addr: usize, _len: usize, _advice: u32) -> KResult<()> {
    Ok(())
}

#[eonix_macros::define_syscall(SYS_MPROTECT)]
async fn mprotect(addr: usize, len: usize, prot: UserMmapProtocol) -> KResult<()> {
    let addr = VAddr::from(addr);
    if !addr.is_page_aligned() || len == 0 {
        return Err(EINVAL);
    }

    let len = len.align_up(PAGE_SIZE);

    thread
        .process
        .mm_list
        .protect(
            addr,
            len,
            Permission {
                read: prot.contains(UserMmapProtocol::PROT_READ),
                write: prot.contains(UserMmapProtocol::PROT_WRITE),
                execute: prot.contains(UserMmapProtocol::PROT_EXEC),
            },
        )
        .await
}

pub fn keep_alive() {}
