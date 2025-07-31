use super::FromSyscallArg;
use crate::fs::shm::{gen_shm_id, ShmFlags, IPC_PRIVATE, SHM_MANAGER};
use crate::kernel::constants::{EBADF, EEXIST, EINVAL, ENOENT, ENOMEM};
use crate::kernel::mem::FileMapping;
use crate::kernel::task::Thread;
use crate::kernel::vfs::filearray::FD;
use crate::{
    kernel::{
        constants::{UserMmapFlags, UserMmapProtocol},
        mem::{Mapping, Permission},
    },
    prelude::*,
};
use align_ext::AlignExt;
use eonix_mm::address::{Addr as _, AddrOps as _, VAddr};
use eonix_mm::paging::PAGE_SIZE;
use eonix_runtime::task::Task;
use posix_types::syscall_no::*;

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

fn do_mmap2(
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
            // The mode is unimportant here, since we are checking prot in mm_area.
            let shared_area = Task::block_on(SHM_MANAGER.lock()).create_shared_area(
                len,
                thread.process.pid,
                0x777,
            );
            Mapping::File(FileMapping::new(shared_area.area.clone(), 0, len))
        }
    } else {
        let file = thread
            .files
            .get(fd)
            .ok_or(EBADF)?
            .get_inode()?
            .ok_or(EBADF)?;

        Mapping::File(FileMapping::new(file, pgoffset, len))
    };

    let permission = Permission {
        read: prot.contains(UserMmapProtocol::PROT_READ),
        write: prot.contains(UserMmapProtocol::PROT_WRITE),
        execute: prot.contains(UserMmapProtocol::PROT_EXEC),
    };

    // TODO!!!: If we are doing mmap's in 32-bit mode, we should check whether
    //          `addr` is above user reachable memory.
    let addr = if flags.contains(UserMmapFlags::MAP_FIXED) {
        Task::block_on(mm_list.unmap(addr, len));
        mm_list.mmap_fixed(addr, len, mapping, permission, is_shared)
    } else {
        mm_list.mmap_hint(addr, len, mapping, permission, is_shared)
    };

    addr.map(|addr| addr.addr())
}

#[cfg(any(target_arch = "riscv64", target_arch = "loongarch64"))]
#[eonix_macros::define_syscall(SYS_MMAP)]
fn mmap(
    addr: usize,
    len: usize,
    prot: UserMmapProtocol,
    flags: UserMmapFlags,
    fd: FD,
    offset: usize,
) -> KResult<usize> {
    do_mmap2(thread, addr, len, prot, flags, fd, offset)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_MMAP2)]
fn mmap2(
    addr: usize,
    len: usize,
    prot: UserMmapProtocol,
    flags: UserMmapFlags,
    fd: FD,
    pgoffset: usize,
) -> KResult<usize> {
    do_mmap2(thread, addr, len, prot, flags, fd, pgoffset)
}

#[eonix_macros::define_syscall(SYS_MUNMAP)]
fn munmap(addr: usize, len: usize) -> KResult<usize> {
    let addr = VAddr::from(addr);
    if !addr.is_page_aligned() || len == 0 {
        return Err(EINVAL);
    }

    let len = len.align_up(PAGE_SIZE);
    Task::block_on(thread.process.mm_list.unmap(addr, len)).map(|_| 0)
}

#[eonix_macros::define_syscall(SYS_BRK)]
fn brk(addr: usize) -> KResult<usize> {
    let vaddr = if addr == 0 { None } else { Some(VAddr::from(addr)) };
    Ok(thread.process.mm_list.set_break(vaddr).addr())
}

#[eonix_macros::define_syscall(SYS_MADVISE)]
fn madvise(_addr: usize, _len: usize, _advice: u32) -> KResult<()> {
    Ok(())
}

#[eonix_macros::define_syscall(SYS_MPROTECT)]
fn mprotect(addr: usize, len: usize, prot: UserMmapProtocol) -> KResult<()> {
    let addr = VAddr::from(addr);
    if !addr.is_page_aligned() || len == 0 {
        return Err(EINVAL);
    }

    let len = len.align_up(PAGE_SIZE);

    Task::block_on(thread.process.mm_list.protect(
        addr,
        len,
        Permission {
            read: prot.contains(UserMmapProtocol::PROT_READ),
            write: prot.contains(UserMmapProtocol::PROT_WRITE),
            execute: prot.contains(UserMmapProtocol::PROT_EXEC),
        },
    ))
}

#[eonix_macros::define_syscall(SYS_SHMGET)]
fn shmget(key: usize, size: usize, shmflg: u32) -> KResult<u32> {
    let size = size.align_up(PAGE_SIZE);

    let mut shm_manager = Task::block_on(SHM_MANAGER.lock());
    let shmid = gen_shm_id(key)?;

    let mode = shmflg & 0o777;
    let shmflg = ShmFlags::from_bits_truncate(shmflg);

    if key == IPC_PRIVATE {
        let new_shm = shm_manager.create_shared_area(size, thread.process.pid, mode);
        shm_manager.insert(shmid, new_shm);
        return Ok(shmid);
    }

    if let Some(_) = shm_manager.get(shmid) {
        if shmflg.contains(ShmFlags::IPC_CREAT | ShmFlags::IPC_EXCL) {
            return Err(EEXIST);
        }

        return Ok(shmid);
    }

    if shmflg.contains(ShmFlags::IPC_CREAT) {
        let new_shm = shm_manager.create_shared_area(size, thread.process.pid, mode);
        shm_manager.insert(shmid, new_shm);
        return Ok(shmid);
    }

    return Err(ENOENT);
}

#[eonix_macros::define_syscall(SYS_SHMAT)]
fn shmat(shmid: u32, addr: usize, shmflg: u32) -> KResult<usize> {
    let mm_list = &thread.process.mm_list;
    let shm_manager = Task::block_on(SHM_MANAGER.lock());
    let shm_area = shm_manager.get(shmid).ok_or(EINVAL)?;

    let mode = shmflg & 0o777;
    let shmflg = ShmFlags::from_bits_truncate(shmflg);

    let mut permission = Permission {
        read: true,
        write: true,
        execute: false,
    };

    if shmflg.contains(ShmFlags::SHM_EXEC) {
        permission.execute = true;
    }
    if shmflg.contains(ShmFlags::SHM_RDONLY) {
        permission.write = false;
    }

    let size = shm_area.shmid_ds.shm_segsz;

    let mapping = Mapping::File(FileMapping {
        file: shm_area.area.clone(),
        offset: 0,
        length: size,
    });

    let addr = if addr != 0 {
        if addr % PAGE_SIZE != 0 && !shmflg.contains(ShmFlags::SHM_RND) {
            return Err(EINVAL);
        }
        let addr = VAddr::from(addr.align_down(PAGE_SIZE));
        mm_list.mmap_fixed(addr, size, mapping, permission, true)
    } else {
        mm_list.mmap_hint(VAddr::NULL, size, mapping, permission, true)
    }?;

    thread.process.shm_areas.lock().insert(addr, size);

    Ok(addr.addr())
}

#[eonix_macros::define_syscall(SYS_SHMDT)]
fn shmdt(addr: usize) -> KResult<usize> {
    let addr = VAddr::from(addr);
    let mut shm_areas = thread.process.shm_areas.lock();
    let size = *shm_areas.get(&addr).ok_or(EINVAL)?;
    shm_areas.remove(&addr);
    drop(shm_areas);
    return Task::block_on(thread.process.mm_list.unmap(addr, size)).map(|_| 0);
}

#[eonix_macros::define_syscall(SYS_SHMCTL)]
fn shmctl(shmid: u32, op: i32, shmid_ds: usize) -> KResult<usize> {
    Ok(0)
}

#[eonix_macros::define_syscall(SYS_MEMBARRIER)]
fn membarrier(_cmd: usize, _flags: usize) -> KResult<()> {
    Ok(())
}

pub fn keep_alive() {}
