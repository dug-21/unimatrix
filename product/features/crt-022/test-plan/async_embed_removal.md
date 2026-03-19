# AsyncEmbedService Removal — Verification Plan

**Component**: `crates/unimatrix-core/src/async_wrappers.rs` (modified — removes AsyncEmbedService)
**Risks addressed**: R-05
**AC addressed**: AC-05

This component's verification is entirely static (grep + cargo). No new unit tests are
required for the removal itself. The existing test suite (all workspace tests) serves as
the negative-test oracle: if any workspace consumer imports `AsyncEmbedService`, compilation
fails, which is the correct failure signal.

---

## §workspace-build — Workspace Compilation (AC-05, R-05)

### Primary Verification

```bash
cargo check --workspace
# Expected: exit code 0
# Any non-zero exit containing "AsyncEmbedService" in the error indicates a workspace consumer
# that was not caught by the grep audit (R-05 scenario 1)
```

The workspace build is authoritative. If it passes, `AsyncEmbedService` has zero consumers.

### Negative Assertion: AsyncEmbedService Absent

```bash
grep -r "AsyncEmbedService" crates/
# Expected: zero results
# Any result is a violation — either the struct was not removed, or a consumer was missed
```

Scope covers all crates: `unimatrix-store`, `unimatrix-vector`, `unimatrix-embed`,
`unimatrix-core`, `unimatrix-server`. The integration harness Python files are excluded
(they do not import Rust types).

---

## §positive-assertion — AsyncVectorStore Retained (AC-05)

```bash
grep -r "AsyncVectorStore" crates/unimatrix-core/
# Expected: at least 2 results:
#   - the struct definition in async_wrappers.rs
#   - the pub use / re-export in lib.rs or async_wrappers.rs
```

`AsyncVectorStore` must not be accidentally removed alongside `AsyncEmbedService`.
The two structs are adjacent in `async_wrappers.rs` (lines 13–83 vs 87–121 in the
pre-removal file). A careless deletion could remove both. This assertion catches that.

```bash
cargo test -p unimatrix-server --lib -- async_vector 2>&1 | tail -10
# Expected: any existing tests for AsyncVectorStore still pass
```

---

## §spawn-blocking-in-core — async_wrappers.rs Cleanup (R-06)

After `AsyncEmbedService` removal:

```bash
grep -n "spawn_blocking" crates/unimatrix-core/src/async_wrappers.rs
# Expected: 5 results — one per AsyncVectorStore method:
#   insert, search, search_filtered, point_count, contains, stale_count, get_embedding
# (7 methods × 1 spawn_blocking each = 7 results — count may vary by implementation)
# Must NOT include the 2 former AsyncEmbedService spawn_blocking calls (embed_entry, embed_entries, dimension)
```

Pre-removal, `async_wrappers.rs` contains `spawn_blocking` at:
- Lines 24, 36, 49, 58, 64, 71, 80 — `AsyncVectorStore` methods (retain)
- Lines 100, 110, 118 — `AsyncEmbedService` methods (remove)

Post-removal, lines 100, 110, 118 must not exist.

---

## §crate-boundary — unimatrix-core Gains No New Dependencies (AC-05)

```bash
cargo tree -p unimatrix-core --depth 1
# Expected: no new dependencies compared to pre-crt-022 baseline
# Specifically: no rayon, no tokio additions (tokio already present via async feature)
```

The removal is a net subtraction: no new code, no new dependencies. The `async` feature
and tokio dependency are retained for `AsyncVectorStore`.

---

## §no-async-embed-in-lib — Public API Surface Check

```bash
grep -n "AsyncEmbedService" crates/unimatrix-core/src/lib.rs
# Expected: zero results
# Pre-removal: AsyncEmbedService may have been re-exported from lib.rs via pub use
```

If `lib.rs` had `pub use async_wrappers::AsyncEmbedService`, that re-export must also
be removed. The workspace build catches this, but an explicit check is cleaner.

---

## §test-binaries — Confirm Test Binaries Do Not Reference AsyncEmbedService

```bash
grep -rn "AsyncEmbedService" crates/unimatrix-server/tests/ 2>/dev/null || echo "no test dir"
grep -rn "AsyncEmbedService" crates/unimatrix-core/tests/ 2>/dev/null || echo "no test dir"
```

R-05 specifically calls out test binaries as a risk: a test binary that imports
`AsyncEmbedService` would fail `cargo test --workspace` even if server code is clean.
These greps confirm no test binary holds a reference.

---

## §expected-state — Post-Removal State of async_wrappers.rs

After removal, `async_wrappers.rs` must contain:

```rust
// Present:
pub struct AsyncVectorStore<T: VectorStore + 'static> { ... }
impl<T: VectorStore + 'static> AsyncVectorStore<T> { ... }
// Methods: new, insert, search, search_filtered, point_count, contains, stale_count, get_embedding

// Absent (removed):
// pub struct AsyncEmbedService<T: EmbedService + 'static> { ... }
// impl<T: EmbedService + 'static> AsyncEmbedService<T> { ... }
// Methods: new, embed_entry, embed_entries, dimension
```

The file header comment (lines 1–4) may be updated to remove the `AsyncEmbedService` reference,
or left as-is if the comment is generic. Either is acceptable — the struct and impl are
the authoritative test targets.

---

## Execution Order

1. Run grep assertions (fast, pre-build)
2. `cargo check --workspace` (type check all crates)
3. `cargo test --workspace 2>&1 | tail -30` (verify no test regression from removal)

The grep assertions catch obvious errors (forgot to delete the struct). The cargo
commands catch consumers the greps might miss (e.g., re-exports, macro expansions).
