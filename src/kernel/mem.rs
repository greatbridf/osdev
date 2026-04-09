pub mod paging;

mod access;
mod address;
mod allocator;
mod folio;
mod mapped;
mod mm_list;
mod page_alloc;
mod page_cache;
mod types;

pub use access::PhysAccess;
pub use folio::{Folio, FolioOwned, LockedFolio};
pub use mapped::AnonFolio;
pub use mm_list::{handle_kernel_page_fault, MMList, Mapping, Permission};
pub use page_alloc::{GlobalPageAlloc, RawPage};
pub use page_cache::{CachePage, PageCache};
pub use paging::PageBuffer;
pub use types::PageOffset;
