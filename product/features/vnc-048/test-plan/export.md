# Test Plan — export.rs (`run_export` / `run_export_with_base` slug branch + stderr summary)

Signature change: add `slug: Option<&str>` to `run_export` / `run_export_with_base` (`export.rs:32,46`).
Branch to `resolve_slug_store` when `Some`; add the AC-06 stderr count summary (both modes).
Tests live in `crates/unimatrix-server/tests/export_integration.rs` (extend, do not fork).
Risks: **R-01 (Critical, top weight)**, R-02, R-06, R-09, R-10, R-13, R-14.

All tests drive `run_export_with_base(...)` — the operator entry point — with `base` pinned to a
TempDir. Reuse the existing `setup_project()` / `run_export_to_string` / `parse_lines` /
`insert_full_entry` helpers.

## Shared seeding helper (define once, reuse in import.md)

`seed_slug_store(base: &Path, slug: &str, ids: &[i64]) -> PathBuf` — mirror the **runtime
`http_provision` literal-slug layout**: `slug_dir = per_slug_data_dir(base, &ProjectSlug::try_from(slug))`,
`SqlxStore::open(slug_dir/"unimatrix.db")`, insert `ids`. This is the **seed path** and MUST be
distinct code from the CLI **read path** (`run_export_with_base(slug=Some(..))` → `resolve_slug_store`).

## R-01 — Disagreement seam (AC-09, TOP weight, gate non-negotiable)

- `test_export_slug_emits_slug_store_not_hash_store` — the mandatory shape:
  1. `base = X` (TempDir); `project_dir` fixed.
  2. Seed `X/<slug>/unimatrix.db` via `seed_slug_store` with **set A** = `{a1,a2,a3}`.
  3. Seed the path-hash store (`ensure_data_directory(project_dir, Some(X)).db_path` = `X/<hash>/...`)
     **differently** with **set B** = `{b1,b2,b3}`, B **non-empty and disjoint** from A.
  4. `run_export_with_base(Some(project_dir), out, base=X, slug=Some("<slug>"), ...)`.
  5. Assert emitted ids `== A` **and** `emitted ∩ B == ∅` (by id + content hash).
  **The test file MUST carry a comment: an N=1 same-path test (B empty, or B seeded through the same
  layout as A) is CEREMONIAL (#4974/#5507) and does NOT satisfy AC-09.** The seed layout and CLI
  resolver are different code; B is non-empty and disjoint by construction.
- `test_export_no_slug_emits_hash_store_divergence_guard` (R-01 S2) — same seeding as above, call
  `run_export_with_base(..., slug=None)`; assert emitted `== B` (the hash store). Proves the two
  paths genuinely diverge and the fixture is not accidentally aliasing one store onto both.

## AC-01 — resolved path + corpus identity

- `test_export_slug_resolves_expected_db_path` — assert the store actually read is
  `{base}/<slug>/unimatrix.db` (emitted rows are the slug store's; corroborates the seam test at the
  single-store level).

## R-02 / R-14 — missing store fails loud, creates nothing (AC-03)

- `test_export_slug_missing_store_fails_loud_fs_unchanged` — valid slug, no `unimatrix.db` at the
  resolved path. Stat the slug dir (and confirm absence) before; call `run_export_with_base(slug=..)`;
  assert: non-zero/`Err`, error message contains the **fully-resolved absolute** db path, **no**
  output file written, and **no** `unimatrix.db`/`vector/`/`-wal`/`-shm` created under `X/<slug>`
  after. `open` is never the gate (SR-02).

## R-06 — host bind-mount fail-loud (AC-03, SR-11/C-7)

- `test_export_slug_host_base_miss_names_resolved_path` — base resolves to a directory where the
  slug store does not exist (host-base-miss simulation); assert fail-loud naming the **resolved
  absolute path** tried; never no-ops, never resolves a different store.

## R-08 — validation at the CLI edge (AC-04)

- `test_export_slug_invalid_rejected_no_fs_touch` — parameterized over charset-invalid + reserved +
  traversal set (see resolve_slug_store.md); driven through `run_export_with_base(slug=Some(bad))`;
  assert rejection **before any FS/DB access**, zero filesystem side effects (no output file, no
  dirs). (The exhaustive reject set is unit-tested in resolve_slug_store.md; here assert the CLI edge
  wires it — a representative subset + one traversal case.)

## R-09 — no-`--slug` fallthrough parity (AC-05, WARN-1)

- `test_export_no_slug_resolved_path_is_path_hash_data_dir` — property assertion: with `slug=None`
  the resolved read path == the path-hash `data_dir` db (funnel not entered). Not one example — assert
  path identity.
- Existing `export_integration.rs` suite passes unchanged (the added `slug` param defaults through
  `None` at existing call sites; if a helper signature changed, update call sites only — C-9).
- **WARN-1 carry (one-line check):** grep `export_integration.rs` for any assertion on empty/absent
  **stderr**; if present, update it to permit the FR-8 summary line (not a regression). AC-05 identity
  is scoped to exported file + stdout + exit code — **stderr excluded**.

## R-10 — stderr count summary (AC-06)

- `test_export_stderr_summary_names_counts_and_path` — capture stderr; assert it contains the entry
  count, audit-row count, and resolved output path (`exported N entries, M audit rows → <path>`);
  assert stdout unaffected. Applies in both slug and no-slug modes (FR-8).
- `test_export_zero_entries_audit_rows_self_diagnoses` — store with 0 knowledge entries but audit
  rows → stderr reads `exported 0 entries, M audit rows` (self-diagnosing sparse export). The
  `--skip-quarantined`/audit asymmetry filter is NOT changed (NFR-9) — an audit-rows-only export
  stays a legitimate output.

## R-13 — stray/hash-dir boundary (AC-11)

- `test_export_no_slug_with_populated_slug_dir_emits_only_hash` — seed both a populated slug dir and
  the hash store under one base; `run_export_with_base(slug=None)` emits only the hash store's rows.
  Boundary guard: a 16-hex hash segment is a charset-valid slug, but no-slug mode never reinterprets
  it (documented, not silent).

## AC-08 — export against a live daemon's slug store (read-only)

- `test_export_slug_readonly_under_wal_writer` — open the slug store read-only while a WAL /
  `busy_timeout` writer context is simulated; assert export succeeds, no locking added (#2621
  open_readonly analogue). Necessary but low-weight relative to the seam.

## Edge cases

- Slug at max (63) / min (1) length with a real store present → resolves and exports.
- `data_dir.parent() == None` propagated from `resolve_slug_store` → fail loud (covered at unit; here
  optional integration sanity).
</content>
