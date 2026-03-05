# Pseudocode: feature-flag-flip

## Changes

### `crates/unimatrix-store/Cargo.toml`

```toml
[features]
default = ["backend-sqlite"]         # NEW: SQLite is now default
test-support = ["dep:tempfile"]
backend-sqlite = ["dep:rusqlite"]

[dependencies]
# Add these two new dependencies:
base64 = "0.22"
serde_json = { workspace = true }
# All existing dependencies remain unchanged
```

### `crates/unimatrix-engine/Cargo.toml`

```toml
# Add a new [features] section:
[features]
backend-sqlite = []                  # Marker feature, no deps
```

### `crates/unimatrix-engine/src/project.rs`

Change the hardcoded `"unimatrix.redb"` to a cfg-gated selection:

```rust
// Before (line 94):
let db_path = data_dir.join("unimatrix.redb");

// After:
#[cfg(feature = "backend-sqlite")]
let db_path = data_dir.join("unimatrix.db");
#[cfg(not(feature = "backend-sqlite"))]
let db_path = data_dir.join("unimatrix.redb");
```

Also update the test `test_ensure_creates_dirs` which asserts the filename ends with `"unimatrix.redb"`:

```rust
// Before:
assert!(paths.db_path.to_string_lossy().ends_with("unimatrix.redb"));

// After:
#[cfg(feature = "backend-sqlite")]
assert!(paths.db_path.to_string_lossy().ends_with("unimatrix.db"));
#[cfg(not(feature = "backend-sqlite"))]
assert!(paths.db_path.to_string_lossy().ends_with("unimatrix.redb"));
```

Also update the `ProjectPaths` doc comment on db_path:

```rust
// Before:
/// Database path: ~/.unimatrix/{hash}/unimatrix.redb
pub db_path: PathBuf,

// After:
/// Database path: ~/.unimatrix/{hash}/unimatrix.db (SQLite) or unimatrix.redb (redb)
pub db_path: PathBuf,
```

### `crates/unimatrix-server/Cargo.toml`

```toml
[features]
default = ["mcp-briefing", "backend-sqlite"]   # Changed: backend-sqlite replaces redb
mcp-briefing = []
backend-sqlite = ["unimatrix-store/backend-sqlite", "unimatrix-engine/backend-sqlite"]  # Updated: propagates to engine too
```

Note: the `redb` feature line stays as-is (optional dep). It just moves out of defaults.

## Compilation Matrix

After these changes:

| Build command | Store backend | Engine db_path | Export | Import |
|---|---|---|---|---|
| `cargo build` (default) | SQLite | unimatrix.db | No | Yes |
| `cargo build --no-default-features --features mcp-briefing` | redb | unimatrix.redb | Yes | No |

## Patterns

This follows the existing cfg-gating pattern in `crates/unimatrix-store/src/lib.rs` where `#[cfg(not(feature = "backend-sqlite"))]` selects redb modules and `#[cfg(feature = "backend-sqlite")]` selects SQLite modules.
