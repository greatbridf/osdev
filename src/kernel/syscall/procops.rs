use core::ffi::CStr;

use alloc::borrow::ToOwned;
use alloc::ffi::CString;
use alloc::sync::Arc;
use bindings::types::elf::{elf32_load, elf32_load_data, ELF_LOAD_FAIL_NORETURN};
use bindings::{
    current_process, current_thread, interrupt_stack, kill_current, mmx_registers, EFAULT, EINVAL,
    ENOENT, ENOTDIR, SIGSEGV,
};

use crate::io::Buffer;
use crate::kernel::user::dataflow::UserString;
use crate::kernel::vfs::dentry::Dentry;
use crate::kernel::vfs::filearray::FileArray;
use crate::path::Path;
use crate::{kernel::user::dataflow::UserBuffer, prelude::*};

use crate::kernel::vfs::{self, FsContext};

use super::{define_syscall32, register_syscall_handler};

fn do_umask(mask: u32) -> KResult<u32> {
    let context = FsContext::get_current();
    let mut umask = context.umask.lock();

    let old = *umask;
    *umask = mask & 0o777;
    Ok(old)
}

fn do_getcwd(buffer: *mut u8, bufsize: usize) -> KResult<usize> {
    let context = FsContext::get_current();
    let mut buffer = UserBuffer::new(buffer, bufsize)?;

    context.cwd.lock().get_path(&context, &mut buffer)?;

    Ok(buffer.wrote())
}

fn do_chdir(path: *const u8) -> KResult<()> {
    let context = FsContext::get_current();
    let path = UserString::new(path)?;
    let path = Path::new(path.as_cstr().to_bytes())?;

    let dentry = Dentry::open(&context, path, true)?;
    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    if !dentry.is_directory() {
        return Err(ENOTDIR);
    }

    *context.cwd.lock() = dentry;
    Ok(())
}

fn do_mount(source: *const u8, target: *const u8, fstype: *const u8, flags: usize) -> KResult<()> {
    let source = UserString::new(source)?;
    let target = UserString::new(target)?;
    let fstype = UserString::new(fstype)?;

    let context = FsContext::get_current();
    let mountpoint = Dentry::open(&context, Path::new(target.as_cstr().to_bytes())?, true)?;
    if !mountpoint.is_valid() {
        return Err(ENOENT);
    }

    vfs::mount::do_mount(
        &mountpoint,
        source.as_cstr().to_str().map_err(|_| EINVAL)?,
        target.as_cstr().to_str().map_err(|_| EINVAL)?,
        fstype.as_cstr().to_str().map_err(|_| EINVAL)?,
        flags as u64,
    )
}

/// # Return
/// `(ip, sp)`
fn do_execve(exec: &[u8], argv: &[CString], envp: &[CString]) -> KResult<(usize, usize)> {
    let context = FsContext::get_current();
    let dentry = Dentry::open(&context, Path::new(exec)?, true)?;
    if !dentry.is_valid() {
        return Err(ENOENT);
    }

    let argv_array = argv.iter().map(|x| x.as_ptr()).collect::<Vec<_>>();
    let envp_array = envp.iter().map(|x| x.as_ptr()).collect::<Vec<_>>();

    let mut load_data = elf32_load_data {
        exec_dent: Arc::into_raw(dentry) as *mut _,
        argv: argv_array.as_ptr(),
        argv_count: argv_array.len(),
        envp: envp_array.as_ptr(),
        envp_count: envp_array.len(),
        ip: 0,
        sp: 0,
    };

    BorrowedArc::<FileArray>::from_raw(
        unsafe { current_process.as_mut() }.unwrap().files.m_handle as *const _,
    )
    .on_exec();

    match unsafe { elf32_load(&mut load_data) } {
        0 => {
            unsafe { current_thread.as_mut().unwrap().signals.on_exec() };
            Ok((load_data.ip, load_data.sp))
        }
        n => {
            if n == ELF_LOAD_FAIL_NORETURN {
                unsafe { kill_current(SIGSEGV as i32) }
            }
            Err(-n as u32)
        }
    }
}

unsafe extern "C" fn sys_execve(
    int_stack: *mut interrupt_stack,
    _mmxregs: *mut mmx_registers,
) -> u32 {
    match (|| -> KResult<()> {
        let exec = int_stack.as_mut().unwrap().regs.rbx as *const u8;
        let exec = UserString::new(exec)?;

        // TODO!!!!!: copy from user
        let mut argv = int_stack.as_mut().unwrap().regs.rcx as *const u32;
        let mut envp = int_stack.as_mut().unwrap().regs.rdx as *const u32;

        if argv.is_null() || envp.is_null() {
            return Err(EFAULT);
        }

        let mut argv_vec = Vec::new();
        let mut envp_vec = Vec::new();

        while argv.read() != 0 {
            argv_vec.push(CStr::from_ptr(argv.read() as *const i8).to_owned());
            argv = argv.add(1);
        }

        while envp.read() != 0 {
            envp_vec.push(CStr::from_ptr(envp.read() as *const i8).to_owned());
            envp = envp.add(1);
        }

        let (ip, sp) = do_execve(exec.as_cstr().to_bytes(), &argv_vec, &envp_vec)?;

        int_stack.as_mut().unwrap().v_rip = ip;
        int_stack.as_mut().unwrap().rsp = sp;
        Ok(())
    })() {
        Ok(_) => 0,
        Err(err) => -(err as i32) as u32,
    }
}

define_syscall32!(sys_chdir, do_chdir, path: *const u8);
define_syscall32!(sys_umask, do_umask, mask: u32);
define_syscall32!(sys_mount, do_mount, source: *const u8, target: *const u8, fstype: *const u8, flags: usize);
define_syscall32!(sys_getcwd, do_getcwd, buffer: *mut u8, bufsize: usize);

pub(super) unsafe fn register() {
    register_syscall_handler(0x0b, sys_execve, b"execve\0".as_ptr() as *const _);
    register_syscall_handler(0x0c, sys_chdir, b"chdir\0".as_ptr() as *const _);
    register_syscall_handler(0x15, sys_mount, b"mount\0".as_ptr() as *const _);
    register_syscall_handler(0x3c, sys_umask, b"umask\0".as_ptr() as *const _);
    register_syscall_handler(0xb7, sys_getcwd, b"getcwd\0".as_ptr() as *const _);
}
