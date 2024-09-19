use crate::bindings::root::EFAULT;
use crate::io::Buffer;
use crate::kernel::mem::phys;
use core::fmt;

pub struct Page {
    page_ptr: *mut crate::bindings::root::kernel::mem::paging::page,
    order: u32,
}

impl Page {
    pub fn alloc_one() -> Self {
        use crate::bindings::root::kernel::mem::paging::alloc_page;
        let page_ptr = unsafe { alloc_page() };

        Self { page_ptr, order: 0 }
    }

    pub fn alloc_many(order: u32) -> Self {
        use crate::bindings::root::kernel::mem::paging::alloc_pages;
        let page_ptr = unsafe { alloc_pages(order) };

        Self { page_ptr, order }
    }

    pub fn len(&self) -> usize {
        1 << (self.order + 12)
    }

    pub fn as_phys(&self) -> usize {
        use crate::bindings::root::kernel::mem::paging::page_to_pfn;

        unsafe { page_to_pfn(self.page_ptr) }
    }

    pub fn as_cached(&self) -> phys::CachedPP {
        phys::CachedPP::new(self.as_phys())
    }

    pub fn as_nocache(&self) -> phys::NoCachePP {
        phys::NoCachePP::new(self.as_phys())
    }

    pub fn zero(&self) {
        use phys::PhysPtr;

        unsafe {
            core::ptr::write_bytes(
                self.as_cached().as_ptr::<u8>(),
                0,
                self.len(),
            );
        }
    }
}

impl Clone for Page {
    fn clone(&self) -> Self {
        unsafe {
            crate::bindings::root::kernel::mem::paging::increase_refcount(
                self.page_ptr,
            );
        }

        Self {
            page_ptr: self.page_ptr,
            order: self.order,
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        unsafe {
            crate::bindings::root::kernel::mem::paging::free_pages(
                self.page_ptr,
                self.order,
            );
        }
    }
}

impl PartialEq for Page {
    fn eq(&self, other: &Self) -> bool {
        assert!(self.page_ptr != other.page_ptr || self.order == other.order);

        self.page_ptr == other.page_ptr
    }
}

unsafe impl Sync for Page {}
unsafe impl Send for Page {}

impl fmt::Debug for Page {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let pfn = self.as_phys();
        write!(f, "Page({:#x}, order={})", pfn, self.order)
    }
}

/// Copy data from a slice to a `Page`
///
/// DONT USE THIS FUNCTION TO COPY DATA TO MMIO ADDRESSES
///
/// # Returns
///
/// Returns `Err(EFAULT)` if the slice is larger than the page
/// Returns `Ok(())` otherwise
pub fn copy_to_page(src: &[u8], dst: &Page) -> Result<(), u32> {
    use phys::PhysPtr;
    if src.len() > dst.len() {
        return Err(EFAULT);
    }

    unsafe {
        core::ptr::copy_nonoverlapping(
            src.as_ptr(),
            dst.as_cached().as_ptr(),
            src.len(),
        );
    }

    Ok(())
}

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
