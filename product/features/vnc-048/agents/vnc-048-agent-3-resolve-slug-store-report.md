# vnc-048 Agent 3 — `resolve_slug_store` funnel (Wave A foundation)

## Summary
Implemented the single shared slug-store resolution funnel `resolve_slug_store` +
`SlugStorePaths`, holding the reuse triad (ADR-001) and the pre-open existence gate
(ADR-002). Placed in `unimatrix-server` (Gate 3a placement correction), factored into a
sibling submodule `projects/slug_store.rs` because `projects.rs` was already at the
500-line ceiling. Raised `per_slug_data_dir` and `ProjectRegistry::validate_slug` to
`pub(crate)` in place. Export/import/main wiring (Wave B/C) depend on these signatures.

## Files modified / created
- `crates/unimatrix-server/src/projects/slug_store.rs` (NEW — funnel + `SlugStorePaths` + 14 unit tests)
- `crates/unimatrix-server/src/projects.rs` (MODIFIED — `pub(crate) mod slug_store;` hook; `per_slug_data_dir` and `ProjectRegistry::validate_slug` raised to `pub(crate)`)

## Public / `pub(crate)` surface added (exact)
```rust
// crates/unimatrix-server/src/projects/slug_store.rs
#[derive(Debug)]
pub(crate) struct SlugStorePaths {
    pub slug_dir: PathBuf,    // {base}/<slug>
    pub db_path: PathBuf,     // slug_dir/PROJECT_DB_NAME ("unimatrix.db")
    pub vector_dir: PathBuf,  // slug_dir/PROJECT_VECTOR_DIR ("vector")
}

pub(crate) fn resolve_slug_store(
    paths: &ProjectPaths,
    raw_slug: &str,
) -> Result<SlugStorePaths, ServerError>;

// crates/unimatrix-server/src/projects.rs — raised in place, bodies unchanged
pub(crate) fn per_slug_data_dir(base: &Path, slug: &ProjectSlug) -> PathBuf;
impl ProjectRegistry { pub(crate) fn validate_slug(raw_slug: &str) -> Result<ProjectSlug, ServerError>; }
```

### Import path for Wave-B consumers (export.rs / import/mod.rs)
```rust
use crate::projects::slug_store::{resolve_slug_store, SlugStorePaths};
```
The submodule is exposed as `pub(crate) mod slug_store;` (NOT a `pub(crate) use` re-export)
to keep the hook in the already-maxed `projects.rs` to a single line. If a leaner
`crate::projects::resolve_slug_store` surface is preferred, add a `pub(crate) use` re-export
in a later wave once `projects.rs` is refactored under the line budget.

## Contract held (verify against ADR-001/002, C-1..C-3)
- ONE base derivation: `paths.data_dir.parent().map(Path::to_path_buf).unwrap_or_else(|| paths.data_dir.clone())` — no `unwrap`/`expect`.
- ONE join site: `per_slug_data_dir(&base, &slug)` — a `&ProjectSlug` (validated newtype) crosses in; a raw `&str` cannot reach it (won't compile).
- ONE validation edge: `ProjectRegistry::validate_slug(raw_slug)?` runs before ANY FS/DB access.
- Constants reused: `PROJECT_DB_NAME` / `PROJECT_VECTOR_DIR` accessed via `super::` (no second literal).
- Existence gate `db_path.exists()` strictly precedes any `SqlxStore::open`; the funnel opens no DB and mutates no filesystem. A miss returns `ServerError::Config` naming the fully-resolved absolute `db_path` + slug + next action.

## Tests
- `cargo test -p unimatrix-server --lib slug_store`: **14 passed / 0 failed**.
- Existing `projects::` lib tests: **60 passed / 0 failed** (visibility raises non-breaking).
- Coverage: 4 deploy-shape base derivations (parent-is-base, `None`-parent fallback no-unwrap, in-container, local-dev); charset/traversal/reserved rejection (each asserts zero FS side effects at the base); boundary lengths (1 and 63); missing-db-before-open + vector-only-is-missing (assert nothing created, slug dir not created); host-base-miss fail-loud with resolved path; `SlugStorePaths` field shape incl. concrete constant values.
- `cargo clippy -p unimatrix-server -- -D warnings`: clean (exit 0). `cargo fmt`: clean.

## Issues / notes for dependent agents
1. **`projects.rs` is 501 lines — 1 over the 500 limit, and structurally unavoidable.** The file was already at EXACTLY 500 (the ceiling) before this change; a foundation submodule requires one `mod` declaration line. All 376 lines of funnel + tests were factored into the sibling `projects/slug_store.rs` (the substance). The residual +1 is a single `pub(crate) mod slug_store;` line. Getting to ≤500 would require deleting an existing line of unrelated vnc-034 code (out of scope / churn). **Flagged for the leader/Gate** — recommend either accepting the +1 or scheduling a small vnc-034 `projects.rs` de-bloat in a later wave.
2. **Two path sources in import slug mode must NOT be tidied (ADR-003/004):** `SlugStorePaths` intentionally omits `pid_path`. Import reads DB/vector from `SlugStorePaths` and PID from the caller's `ProjectPaths.pid_path` (base-scoped). Do not add a `pid_path` to `SlugStorePaths`.
3. **`ServerError` is a large enum** — matching only `ServerError::Config(_)` is non-exhaustive; tests use `let ServerError::Config(msg) = err else { panic!(...) };`. Wave-B callers using `.map_err`/`?` are unaffected.
4. **`cargo fmt -p unimatrix-server` reformats the whole crate** and dirtied two out-of-scope files already fmt-noncompliant on the branch (`mcp/edge_write_delete_agent_tests.rs`, `tests/project_routing_integration.rs`); reverted both before returning. Wave-B agents should `git status` and revert such churn before committing.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (task = resolve_slug_store funnel) — surfaced ADR-001 (#5693, reuse triad), the vnc-034 per-slug-sibling gotcha (#4972), path-hash isolation (#80), and the personal-cloud per-slug-store capability (#5591). Confirmed the design; applied constant-reuse (`super::PROJECT_DB_NAME`/`PROJECT_VECTOR_DIR`) and the no-unwrap base idiom.
- Stored: nothing novel to store. The funnel contract is fully captured in ADR-001/ADR-002 + the component pseudocode; the implementation traps encountered (non-exhaustive `ServerError` → `let-else` in tests; child-module `super::` access to private consts; crate-wide `cargo fmt` churn) are standard Rust or already-recorded lessons ("Revert fmt churn before wave commits"). No reusable gotcha invisible in source was discovered.
