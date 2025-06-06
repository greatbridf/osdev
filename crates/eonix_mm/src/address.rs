mod addr;
mod addr_range;
mod error;
mod paddr;
mod vaddr;

pub use addr::{Addr, AddrOps};
pub use addr_range::AddrRange;
pub use error::AddressError;
pub use paddr::{PAddr, PhysAccess};
pub use vaddr::VAddr;

pub type PRange = AddrRange<PAddr>;
pub type VRange = AddrRange<VAddr>;
