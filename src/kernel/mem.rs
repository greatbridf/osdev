pub mod paging;
pub mod phys;

mod address;
mod mm_area;
mod mm_list;
mod page_alloc;
mod page_table;

#[allow(unused_imports)]
pub use address::{PAddr, VAddr, VRange, PFN, VPN};
pub(self) use mm_area::MMArea;
pub use mm_list::{handle_page_fault, FileMapping, MMList, Mapping, Permission};
pub(self) use page_table::{PageTable, PTE};
pub use paging::{Page, PageBuffer};
