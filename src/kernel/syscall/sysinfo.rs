use crate::{
    kernel::{
        constants::{CLOCK_MONOTONIC, CLOCK_REALTIME, EINVAL},
        task::Thread,
        timer::ticks,
        user::UserPointerMut,
    },
    prelude::*,
};
use posix_types::syscall_no::*;

#[derive(Clone, Copy)]
struct NewUTSName {
    sysname: [u8; 65],
    nodename: [u8; 65],
    release: [u8; 65],
    version: [u8; 65],
    machine: [u8; 65],
    domainname: [u8; 65],
}

fn copy_cstr_to_array(cstr: &[u8], array: &mut [u8]) {
    let len = cstr.len().min(array.len() - 1);
    array[..len].copy_from_slice(&cstr[..len]);
    array[len] = 0;
}

#[eonix_macros::define_syscall(SYS_NEWUNAME)]
fn newuname(buffer: *mut NewUTSName) -> KResult<()> {
    let buffer = UserPointerMut::new(buffer)?;
    let mut uname = NewUTSName {
        sysname: [0; 65],
        nodename: [0; 65],
        release: [0; 65],
        version: [0; 65],
        machine: [0; 65],
        domainname: [0; 65],
    };

    // Linux compatible
    copy_cstr_to_array(b"Linux", &mut uname.sysname);
    copy_cstr_to_array(b"(none)", &mut uname.nodename);
    copy_cstr_to_array(b"1.0.0", &mut uname.release);
    copy_cstr_to_array(b"1.0.0", &mut uname.version);
    copy_cstr_to_array(b"x86", &mut uname.machine);
    copy_cstr_to_array(b"(none)", &mut uname.domainname);

    buffer.write(uname)
}

#[allow(dead_code)]
#[derive(Default, Clone, Copy)]
pub struct TimeVal {
    sec: u64,
    usec: u64,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct TimeSpec {
    sec: u64,
    nsec: u64,
}

#[eonix_macros::define_syscall(SYS_GETTIMEOFDAY)]
fn gettimeofday(timeval: *mut TimeVal, timezone: *mut ()) -> KResult<()> {
    if !timezone.is_null() {
        return Err(EINVAL);
    }

    if !timeval.is_null() {
        let timeval = UserPointerMut::new(timeval)?;
        let ticks = ticks();
        timeval.write(TimeVal {
            sec: ticks.in_secs() as u64,
            usec: ticks.in_usecs() as u64 % 1_000_000,
        })?;
    }

    Ok(())
}

fn do_clock_gettime64(_thread: &Thread, clock_id: u32, timespec: *mut TimeSpec) -> KResult<()> {
    if clock_id != CLOCK_REALTIME && clock_id != CLOCK_MONOTONIC {
        unimplemented!("Unsupported clock_id: {}", clock_id);
    }

    let timespec = UserPointerMut::new(timespec)?;
    let ticks = ticks();
    timespec.write(TimeSpec {
        sec: ticks.in_secs() as u64,
        nsec: ticks.in_nsecs() as u64 % 1_000_000_000,
    })
}

#[cfg(not(target_arch = "x86_64"))]
#[eonix_macros::define_syscall(SYS_CLOCK_GETTIME)]
fn clock_gettime(clock_id: u32, timespec: *mut TimeSpec) -> KResult<()> {
    do_clock_gettime64(thread, clock_id, timespec)
}

#[cfg(target_arch = "x86_64")]
#[eonix_macros::define_syscall(SYS_CLOCK_GETTIME64)]
fn clock_gettime64(clock_id: u32, timespec: *mut TimeSpec) -> KResult<()> {
    do_clock_gettime64(thread, clock_id, timespec)
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Sysinfo {
    uptime: u32,
    loads: [u32; 3],
    totalram: u32,
    freeram: u32,
    sharedram: u32,
    bufferram: u32,
    totalswap: u32,
    freeswap: u32,
    procs: u16,
    totalhigh: u32,
    freehigh: u32,
    mem_unit: u32,
    _padding: [u8; 8],
}

#[eonix_macros::define_syscall(SYS_SYSINFO)]
fn sysinfo(info: *mut Sysinfo) -> KResult<()> {
    let info = UserPointerMut::new(info)?;
    info.write(Sysinfo {
        uptime: ticks().in_secs() as u32,
        loads: [0; 3],
        totalram: 100,
        freeram: 50,
        sharedram: 0,
        bufferram: 0,
        totalswap: 0,
        freeswap: 0,
        procs: 10,
        totalhigh: 0,
        freehigh: 0,
        mem_unit: 1024,
        _padding: [0; 8],
    })
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TMS {
    tms_utime: u32,
    tms_stime: u32,
    tms_cutime: u32,
    tms_cstime: u32,
}

#[eonix_macros::define_syscall(SYS_TIMES)]
fn times(tms: *mut TMS) -> KResult<()> {
    let tms = UserPointerMut::new(tms)?;
    tms.write(TMS {
        tms_utime: 0,
        tms_stime: 0,
        tms_cutime: 0,
        tms_cstime: 0,
    })
}

pub fn keep_alive() {}
