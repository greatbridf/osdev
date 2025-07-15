pub mod paging;

mod access;
mod address;
mod allocator;
mod mm_area;
mod mm_list;
mod page_alloc;
mod page_cache;

pub use access::{AsMemoryBlock, MemoryBlock, PhysAccess};
pub(self) use mm_area::MMArea;
pub use mm_list::{handle_kernel_page_fault, FileMapping, MMList, Mapping, Permission};
pub use page_alloc::{GlobalPageAlloc, RawPage};
pub use page_cache::{CachePage, CachePageStream, PageCache, PageCacheBackend};
pub use paging::{Page, PageBuffer};
