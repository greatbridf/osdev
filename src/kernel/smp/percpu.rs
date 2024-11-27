use crate::kernel::mem::{paging::Page, phys::PhysPtr as _};

/// # Safety
/// The memory allocated by this function will never be freed and can only be used
/// for per-cpu area.
pub unsafe fn alloc_percpu_area() -> *mut () {
    extern "C" {
        static PERCPU_PAGES: usize;
        fn _PERCPU_DATA_START();
    }
    assert_eq!(
        unsafe { PERCPU_PAGES },
        1,
        "We support only 1 page per-cpu variables for now"
    );

    let page = Page::alloc_one();
    unsafe {
        page.as_cached()
            .as_ptr::<u8>()
            .copy_from_nonoverlapping(_PERCPU_DATA_START as *const _, page.len())
    };

    let addr = page.as_cached().as_ptr();
    core::mem::forget(page);

    addr
}

pub unsafe fn set_percpu_area(area: *mut ()) {
    arch::set_percpu_area_thiscpu(area)
}
