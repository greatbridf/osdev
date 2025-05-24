pub mod paging;

mod access;
mod address;
mod mm_area;
mod mm_list;
mod page_alloc;

pub use access::{AsMemoryBlock, MemoryBlock, PhysAccess};
pub(self) use mm_area::MMArea;
pub use mm_list::{handle_page_fault, FileMapping, MMList, Mapping, Permission};
pub use page_alloc::GlobalPageAlloc;
pub use paging::{Page, PageBuffer};
