#![cfg_attr(not(feature = "std"), no_std)]
#![feature(arbitrary_self_types)]
#![feature(dispatch_from_dyn)]
#![feature(unsize)]

mod arcref;

pub use arcref::{ArcRef, AsArcRef};
