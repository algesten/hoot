#![cfg_attr(all(not(feature = "std"), not(test)), no_std)]

mod router;
pub use router::{Router, Service};
