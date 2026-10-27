#![no_std]

use core::ptr::NonNull;

pub struct List {
    head: Link,
    count: usize,
}

pub struct Link {
    prev: Option<NonNull<Link>>,
    next: Option<NonNull<Link>>,
}

impl List {
    pub const fn new() -> Self {
        Self {
            head: Link::new(),
            count: 0,
        }
    }

    pub const fn count(&self) -> usize {
        self.count
    }

    pub fn insert(&mut self, node: &mut Link) {
        // TODO: `node` above should be of 'static.

        self.head.insert(node);
        self.count += 1;
    }

    pub fn remove(&mut self, node: &mut Link) {
        node.remove();
        self.count -= 1;
    }

    pub fn pop(&mut self) -> Option<&mut Link> {
        self.head.next_mut().map(|node| {
            self.count -= 1;
            node.remove();
            node
        })
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn head(&mut self) -> Option<&mut Link> {
        self.head.next_mut()
    }
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
            let insert_node = NonNull::new(&raw mut *node);
            if let Some(next) = self.next {
                (*next.as_ptr()).prev = insert_node;
            }
            node.next = self.next;
            node.prev = NonNull::new(&raw mut *self);
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
        ::core::ptr::NonNull::new_unchecked(ptr.sub(offset) as *mut $type)
    }}
}
