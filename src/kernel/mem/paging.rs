use crate::io::Buffer;

#[repr(C)]
struct ZoneInfo {
    head: *mut core::ffi::c_void,
    count: core::ffi::c_size_t,
}

fn do_dump_buddy(
    zones: &[ZoneInfo],
    buffer: &mut Buffer,
) -> Result<usize, core::fmt::Error> {
    let maxi = {
        let mut maxi = 0;
        for (idx, zone) in zones.iter().enumerate() {
            if zone.count > 0 {
                maxi = idx;
            }
        }
        maxi
    };

    use core::fmt::Write;
    write!(buffer, "Order")?;

    for idx in 0..=maxi {
        write!(buffer, "\t{}", idx)?;
    }

    write!(buffer, "\nCount")?;

    for zone in zones.iter().take(maxi + 1) {
        write!(buffer, "\t{}", zone.count)?;
    }

    write!(buffer, "\n")?;

    Ok(buffer.count())
}

#[no_mangle]
extern "C" fn real_dump_buddy(
    zones: *const ZoneInfo,
    buf: *mut core::ffi::c_uchar,
    buf_size: core::ffi::c_size_t,
) -> core::ffi::c_ssize_t {
    let zones = unsafe { core::slice::from_raw_parts(zones, 52) };
    let mut buffer = Buffer::new(buf, buf_size);

    use crate::bindings::root::ENOMEM;
    match do_dump_buddy(zones, &mut buffer) {
        Ok(size) => size as core::ffi::c_ssize_t,
        Err(_) => -(ENOMEM as core::ffi::c_ssize_t),
    }
}
