#![cfg_attr(all(not(feature = "std"), not(test)), no_std)]

mod url;
pub use url::{Url, UrlError};
