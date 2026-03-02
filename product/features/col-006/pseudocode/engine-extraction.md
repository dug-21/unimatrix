# Pseudocode: engine-extraction

## Purpose

Extract `project.rs`, `confidence.rs`, and `coaccess.rs` from `unimatrix-server` into a new `unimatrix-engine` crate. This is the highest-risk component (R-01, SR-01). The extraction is purely structural -- no logic changes. After extraction, extend `ProjectPaths` with `socket_path`.

## New Crate Setup

### crates/unimatrix-engine/Cargo.toml

```
[package]
name = "unimatrix-engine"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
unimatrix-core = { path = "../unimatrix-core" }
unimatrix-store = { path = "../unimatrix-store" }
serde = { workspace = true }
serde_json = "1"
sha2 = "0.10"
dirs = "6"
tracing = "0.1"

[dev-dependencies]
tempfile = "3"
```

### crates/unimatrix-engine/src/lib.rs

```
#![forbid(unsafe_code)]

pub mod project;
pub mod confidence;
pub mod coaccess;
pub mod wire;
pub mod transport;
pub mod auth;
pub mod event_queue;
```

## Extraction Step 1: project.rs

### Move

Copy `crates/unimatrix-server/src/project.rs` to `crates/unimatrix-engine/src/project.rs` verbatim, including all tests.

### Extend ProjectPaths

Add `socket_path` field after `pid_path`:

```
pub struct ProjectPaths {
    pub project_root: PathBuf,
    pub project_hash: String,
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub vector_dir: PathBuf,
    pub pid_path: PathBuf,
    pub socket_path: PathBuf,  // NEW
}
```

In `ensure_data_directory`, compute socket_path:

```
let socket_path = data_dir.join("unimatrix.sock");
```

Add socket_path to the returned ProjectPaths struct. No directory creation needed for the socket file itself (it is created by bind()).

### Update existing test

In `test_ensure_creates_dirs`, add assertion:

```
assert!(paths.socket_path.to_string_lossy().ends_with("unimatrix.sock"));
```

### Remove from server

Delete `crates/unimatrix-server/src/project.rs`.

### Re-export from server

In `crates/unimatrix-server/src/lib.rs`, replace `pub mod project;` with:

```
pub use unimatrix_engine::project;
```

### Add dependency

In `crates/unimatrix-server/Cargo.toml`, add:

```
unimatrix-engine = { path = "../unimatrix-engine" }
```

### Workspace

In root `Cargo.toml`, verify `members = ["crates/*"]` covers the new crate (it does, via glob).

### Gate: Run full test suite. All 1199 tests pass.

## Extraction Step 2: confidence.rs

### Move

Copy `crates/unimatrix-server/src/confidence.rs` to `crates/unimatrix-engine/src/confidence.rs` verbatim, including all tests and constants.

The module references `coaccess::MAX_MEANINGFUL_PARTNERS` in `co_access_affinity()`. During the intermediate state (confidence moved, coaccess not yet moved):
- Move the `MAX_MEANINGFUL_PARTNERS` constant to `confidence.rs` temporarily as a local constant
- OR: move both confidence and coaccess at the same time in this step

Recommended approach: move both simultaneously since they have a mutual reference and live in the same destination crate. This avoids the intermediate broken state.

### If moving both simultaneously:

Move both `confidence.rs` and `coaccess.rs` to `unimatrix-engine/src/` in one step. Update the cross-reference from `coaccess::MAX_MEANINGFUL_PARTNERS` to a crate-local reference (both modules are now in the same crate).

### Remove from server

Delete `crates/unimatrix-server/src/confidence.rs`.

### Re-export from server

In `crates/unimatrix-server/src/lib.rs`, replace `pub mod confidence;` with:

```
pub use unimatrix_engine::confidence;
```

### Gate: Run full test suite. All 1199 tests pass.

## Extraction Step 3: coaccess.rs

If not moved simultaneously with confidence.rs:

### Move

Copy `crates/unimatrix-server/src/coaccess.rs` to `crates/unimatrix-engine/src/coaccess.rs`.

### Fix cross-reference

The coaccess module depends on `unimatrix_store::Store` for `get_co_access_partners()`. This dependency is satisfied by `unimatrix-engine`'s dependency on `unimatrix-store`.

Update import paths inside coaccess.rs:
- `use crate::confidence::co_access_affinity` becomes valid (both in engine crate)
- `use unimatrix_store::Store` remains unchanged

### Remove from server

Delete `crates/unimatrix-server/src/coaccess.rs`.

### Re-export from server

In `crates/unimatrix-server/src/lib.rs`, replace `pub mod coaccess;` with:

```
pub use unimatrix_engine::coaccess;
```

### Gate: Run full test suite. All 1199 tests pass.

## Post-Extraction: Server lib.rs

Final state of `crates/unimatrix-server/src/lib.rs`:

```
#![forbid(unsafe_code)]

pub use unimatrix_engine::project;
pub use unimatrix_engine::confidence;
pub use unimatrix_engine::coaccess;

pub mod audit;
pub mod categories;
pub mod coherence;
pub mod contradiction;
pub mod embed_handle;
pub mod error;
pub mod identity;
pub mod outcome_tags;
pub mod pidfile;
pub mod registry;
pub mod response;
pub mod scanning;
pub mod server;
pub mod shutdown;
pub mod tools;
pub mod usage_dedup;
pub mod validation;
pub mod hook;          // NEW
pub mod uds_listener;  // NEW
```

## Post-Extraction: Verify No Stale Files

Confirm that NO `confidence.rs`, `coaccess.rs`, or `project.rs` source files remain in `crates/unimatrix-server/src/`. Only re-exports in `lib.rs`.

## Error Handling

- If any test fails after an extraction step, STOP. Do not proceed to the next module.
- If imports fail to resolve, check that unimatrix-engine's dependencies include the required crates.
- If the re-export path `unimatrix_server::confidence` does not resolve, verify the `pub use` syntax is module-level (not individual function re-exports).

## Key Test Scenarios

1. After full extraction: `cargo test --workspace` passes all 1199 tests
2. `unimatrix_engine::confidence::compute_confidence` callable from engine crate
3. `unimatrix_server::confidence::compute_confidence` resolves via re-export
4. `ProjectPaths` has `socket_path` field set to `data_dir.join("unimatrix.sock")`
5. No source files for extracted modules remain in server src/
