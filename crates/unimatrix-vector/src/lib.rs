#![forbid(unsafe_code)]

mod config;
mod error;
mod filter;
mod index;
mod persistence;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;

pub use config::VectorConfig;
pub use error::{Result, VectorError};
pub use index::{SearchResult, VectorIndex};
