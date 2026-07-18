# Test Plan — `resolve_slug_store` funnel (projects.rs)

Component: the single slug-store resolution funnel + `SlugStorePaths` struct; `pub(crate)` raises of
`per_slug_data_dir` and `ProjectRegistry::validate_slug`. Owns the reuse triad (ADR-001) and the
pre-open existence gate (ADR-002). Risks: R-02, R-05, R-06, R-08, R-13, R-14.

These are **unit** tests in `projects.rs` `#[cfg(test)]`. They prove funnel *shape* — ordering,
structural rejection, no-`unwrap`. They are necessary but do NOT discharge any gate AC on their own;
the gate ACs drive real `run_*` entry points (export.md / import.md).

## Funnel order (the core contract) — FR-5, C-3

`validate_slug(raw)` → `base = data_dir.parent()` (fallback, no unwrap) →
`slug_dir = per_slug_data_dir(base, &slug)` → `db_path = slug_dir/"unimatrix.db"` →
**existence gate `db_path.exists()`** → (import only) live-PID → non-empty-audit → `SqlxStore::open`.

## Test expectations

### Base derivation (R-05, C-1, NFR-4) — AC-01

- `test_resolve_slug_store_base_is_data_dir_parent` — given `ProjectPaths{ data_dir: X/<hash>, .. }`,
  assert returned `slug_dir == X/<slug>` (base == `data_dir.parent()`). Proves the `_with_base`
  wrinkle at the unit level.
- `test_resolve_slug_store_parent_none_uses_fallback_no_unwrap` — construct `ProjectPaths` whose
  `data_dir` is a filesystem root (`.parent() == None`); assert the fallback idiom
  (`unwrap_or_else(|| data_dir.clone())`) is used and the call does **not** panic. Guards NFR-4 (no
  `unwrap`/`expect` in non-test code). Existence gate then fails loud (no store there) — assert `Err`.
- `test_resolve_slug_store_in_container_shape_derivation` — with `data_dir == /data/.unimatrix/<hash>`
  style input, assert `slug_dir.parent() == /data/.unimatrix` (in-container shape 1, no container in
  CI). Derivation-unit assertion for the deploy-shape axis.
- `test_resolve_slug_store_local_dev_shape_derivation` — `data_dir` under a `~/.unimatrix`-style base
  → `slug_dir.parent() == base` (shape 2).

### Validation edge (R-08, C-2, NFR-8) — AC-04

Validation runs **before any FS/DB access**. `validate_slug` = `ProjectSlug::try_from` (charset) +
`is_reserved_slug` (reserved), two separate checks.

- `test_resolve_slug_store_rejects_charset_invalid` — parameterized over `Foo!`, `a_b`, `-lead`,
  `UPPER`, 64-byte string → `Err`, and assert **no directory/file created** under base (stat base
  children before/after). Charset closure is structural at `ProjectSlug::try_from`.
- `test_resolve_slug_store_rejects_traversal` — parameterized over `../x`, `..`, `%2e%2e`, `/abs`,
  `a/b`, `a\\b`, embedded-NUL (`a\0b`) → `Err` at `ProjectSlug::try_from` (unrepresentable), **zero
  filesystem touch**. Any code path admitting a raw `&str` into `per_slug_data_dir` is a finding.
- `test_resolve_slug_store_rejects_reserved` — parameterized over `v1`, `health`, `observe`, `tools`
  → `Err` before FS/DB. Includes the reserved-name-that-collides-with-a-real-hash-dir edge: reserved
  check still rejects at the edge.
- `test_resolve_slug_store_accepts_boundary_lengths` — 1-byte and 63-byte valid slugs pass
  validation (then hit the existence gate). Boundary of `ProjectSlug::try_from` (1..=63 ASCII).
- Structural guard: `per_slug_data_dir` accepts only `&ProjectSlug`. No unit test can pass a `&str`
  (won't compile) — call this out in a code comment; it is the NFR-8 proof by construction.

### Existence gate before `open` (R-02, C-3) — AC-03

- `test_resolve_slug_store_missing_db_errors_before_open` — valid slug, no `unimatrix.db` at the
  resolved path → `Err` whose message contains the **fully-resolved absolute** `db_path`. Assert
  **nothing created**: no `unimatrix.db`, no `vector/`, no `-wal`/`-shm` under `X/<slug>` (stat the
  slug dir before/after; note `ensure_data_directory` may have created the *hash* dir per C-6 — that
  is not the slug dir and must not be conflated, R-13/integration-risk).
- `test_resolve_slug_store_vector_only_dir_is_missing_store` — `X/<slug>/vector/` exists but
  `unimatrix.db` absent → treated as missing store (gate is on the db file). `Err`, fail loud.
- Ordering assertion: the funnel returns `Err` on a nonexistent slug **without** `SqlxStore::open`
  ever being called (open auto-creates+migrates = a write; SR-02). Prove structurally — the existence
  check precedes the `open` call site; a nonexistent slug leaves **no** migrated db on disk (the
  `test_resolve_slug_store_missing_db_errors_before_open` FS-unchanged assertion is the observable
  proof).

### Host bind-mount fail-loud corner (R-06, SR-11, C-7) — AC-03

- `test_resolve_slug_store_host_base_miss_fails_loud_with_resolved_path` — base derives to a
  directory where the slug store does NOT exist (simulating host `$HOME/.unimatrix` ≠ container
  base) → `Err` naming the **resolved absolute path** actually tried (which distinguishes a base miss
  from a typo). Assert it never returns `Ok` and never resolves a different (container) store. The
  integration side of this corner (driving `run_export_with_base`) lives in export.md.

## `SlugStorePaths` shape

- `test_slug_store_paths_fields` — on success, `db_path == slug_dir/"unimatrix.db"` and
  `vector_dir == slug_dir/"vector"` (PROJECT_VECTOR_DIR). Confirms import's redirect target is
  derived here, not re-joined downstream.

## Coverage note

Every rejection/failure path asserts **zero filesystem side effects at the slug dir** (R-14). These
units feed the integration FS-unchanged assertions (AC-03) but do not replace them — the operator
outcome is proven by driving `run_export_with_base`/`run_import_with_base` against a nonexistent slug.
</content>
