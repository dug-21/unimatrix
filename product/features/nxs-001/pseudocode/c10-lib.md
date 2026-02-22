# C10: Lib (Crate Root) Pseudocode

## Purpose

Crate root with `#![forbid(unsafe_code)]`, module declarations, and public re-exports.

## Module: lib.rs

```
#![forbid(unsafe_code)]

mod schema;
mod error;
mod db;
mod counter;
mod write;
mod read;
mod query;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;

pub use schema::{EntryRecord, Status, NewEntry, QueryFilter, TimeRange, DatabaseConfig};
pub use db::Store;
pub use error::{StoreError, Result};
```

## Notes

- All functionality is exposed through methods on `Store` and the schema types.
- Internal modules (counter, write, read, query) have `pub(crate)` items but no direct public re-exports beyond what Store methods provide.
- The `test_helpers` module is only available when running tests or when the `test-support` feature is enabled.
- `#![forbid(unsafe_code)]` at crate root applies to the entire crate (NFR-04, AC-15).
