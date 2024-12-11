use core::ptr::NonNull;

pub struct Link {
    prev: Option<NonNull<Link>>,
    next: Option<NonNull<Link>>,
}

impl Link {
    pub const fn new() -> Self {
        Self {
            prev: None,
            next: None,
        }
    }

    pub fn insert(&mut self, node: &mut Self) {
        unsafe {
            let insert_node = NonNull::new(node as *mut Self);
            if let Some(next) = self.next {
                (*next.as_ptr()).prev = insert_node;
            }
            node.next = self.next;
            node.prev = NonNull::new(self as *mut Self);
            self.next = insert_node;
        }
    }

    pub fn remove(&mut self) {
        if let Some(next) = self.next {
            unsafe { (*next.as_ptr()).prev = self.prev };
        }

        if let Some(prev) = self.prev {
            unsafe { (*prev.as_ptr()).next = self.next };
        }

        self.prev = None;
        self.next = None;
    }

    pub fn next(&self) -> Option<&Self> {
        self.next.map(|node| unsafe { &*node.as_ptr() })
    }

    pub fn next_mut(&mut self) -> Option<&mut Self> {
        self.next.map(|node| unsafe { &mut *node.as_ptr() })
    }
}

#[macro_export]
macro_rules! container_of {
    ($ptr:expr, $type:ty, $($f:tt)*) => {{
        let ptr = $ptr as *const _ as *const u8;
        let offset: usize = ::core::mem::offset_of!($type, $($f)*);
        ptr.sub(offset) as *mut $type
    }}
}

#[allow(unused_imports)]
pub use container_of;
