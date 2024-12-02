pub mod paging;
pub mod phys;

mod mm_area;
mod mm_list;
mod page_table;
mod page_alloc;
mod address;

pub(self) use mm_area::MMArea;
pub use mm_list::{handle_page_fault, FileMapping, MMList, Mapping, PageFaultError, Permission};
pub(self) use page_table::{PageTable, PTE};
pub use address::{VAddr, PAddr, VPN, PFN, VRange};
pub use page_alloc::{alloc_page, alloc_pages, free_pages, mark_present, create_pages};
