mod arcswap;
mod condvar;

pub use arcswap::ArcSwap;
pub use eonix_sync::Spin;

pub type CondVar = condvar::CondVar<true>;
