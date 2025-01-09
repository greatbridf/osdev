use bindings::EINVAL;

use crate::{
    kernel::{
        constants::{CLOCK_MONOTONIC, CLOCK_REALTIME},
        timer::ticks,
        user::UserPointerMut,
    },
    prelude::*,
};

use super::{define_syscall32, register_syscall};

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

fn do_newuname(buffer: *mut NewUTSName) -> KResult<()> {
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
#[derive(Clone, Copy)]
struct TimeVal {
    sec: u64,
    usec: u64,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
struct TimeSpec {
    sec: u64,
    nsec: u64,
}

fn do_gettimeofday(timeval: *mut TimeVal, timezone: *mut ()) -> KResult<()> {
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

fn do_clock_gettime64(clock_id: u32, timespec: *mut TimeSpec) -> KResult<()> {
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

define_syscall32!(sys_newuname, do_newuname, buffer: *mut NewUTSName);
define_syscall32!(sys_gettimeofday, do_gettimeofday, timeval: *mut TimeVal, timezone: *mut ());
define_syscall32!(sys_clock_gettime64, do_clock_gettime64, clock_id: u32, timespec: *mut TimeSpec);

pub(super) fn register() {
    register_syscall!(0x4e, gettimeofday);
    register_syscall!(0x7a, newuname);
    register_syscall!(0x193, clock_gettime64);
}
