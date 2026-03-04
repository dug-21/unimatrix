# Pseudocode: infra-migration

## Purpose
Move 13 flat infrastructure modules from `src/` to `src/infra/`, create `infra/mod.rs` with re-exports, update `lib.rs`.

## Files Created
- `src/infra/mod.rs`

## Files Moved (git mv)
- `src/audit.rs` -> `src/infra/audit.rs`
- `src/registry.rs` -> `src/infra/registry.rs`
- `src/session.rs` -> `src/infra/session.rs`
- `src/scanning.rs` -> `src/infra/scanning.rs`
- `src/validation.rs` -> `src/infra/validation.rs`
- `src/categories.rs` -> `src/infra/categories.rs`
- `src/contradiction.rs` -> `src/infra/contradiction.rs`
- `src/coherence.rs` -> `src/infra/coherence.rs`
- `src/pidfile.rs` -> `src/infra/pidfile.rs`
- `src/shutdown.rs` -> `src/infra/shutdown.rs`
- `src/embed_handle.rs` -> `src/infra/embed_handle.rs`
- `src/usage_dedup.rs` -> `src/infra/usage_dedup.rs`
- `src/outcome_tags.rs` -> `src/infra/outcome_tags.rs`

## Files Modified
- `src/lib.rs`

## Pseudocode

### src/infra/mod.rs

```
// Re-export all infrastructure modules for crate-internal use.
// Public for integration tests (matching existing visibility).

pub mod audit;
pub mod categories;
pub mod coherence;
pub mod contradiction;
pub mod embed_handle;
pub mod outcome_tags;
pub mod pidfile;
pub mod registry;
pub mod scanning;
pub mod session;
pub mod shutdown;
pub mod usage_dedup;
pub mod validation;
```

### src/lib.rs changes

Replace the 13 individual `pub mod X;` declarations with:
```
pub mod infra;
```

Add temporary re-exports for backward compatibility during migration:
```
// Temporary re-exports — removed in cleanup step
pub use infra::audit;
pub use infra::categories;
pub use infra::coherence;
pub use infra::contradiction;
pub use infra::embed_handle;
pub use infra::outcome_tags;
pub use infra::pidfile;
pub use infra::registry;
pub use infra::scanning;
pub use infra::session;
pub use infra::shutdown;
pub use infra::usage_dedup;
pub use infra::validation;
```

Keep unchanged: `pub mod error;`, `pub mod server;`, `pub mod services;`, `pub mod tools;`, `pub mod identity;`, `pub mod response;`, `pub mod hook;`, `pub mod uds_listener;`.

### Internal import updates

All `use crate::audit::*` references in files outside infra/ stay working via re-exports initially. Final cleanup updates them to `use crate::infra::audit::*`.

### Module content

No changes to any moved module's content. Each file is `git mv`'d as-is. The `#[cfg(test)] mod tests` blocks move with their modules.

## Compilation Gate

After this step: `cargo check --workspace` must succeed. `cargo test --workspace` must pass with same count.

## Deviation from Existing Patterns

None. This is a standard module group extraction.
