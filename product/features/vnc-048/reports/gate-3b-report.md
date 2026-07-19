# Gate 3b Report: vnc-048

> Gate: 3b (Code Review)
> Date: 2026-07-19
> Validator: vnc-048-gate-3b
> Commit: fdf932e7 (HEAD, branch feature/vnc-048)
> Result: **PASS**

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | Funnel + export/import branches match pseudocode step-for-step; count derivation follows pattern #5707 (written = total − skipped). |
| 2. Architecture compliance | PASS | Single `resolve_slug_store` funnel; ADR-001..006 honored; all hard invariants hold in committed code. |
| 3. Interface implementation | PASS | Funnel signature, `run_export*/run_import*` +slug threading, clap `--slug` all as specified. |
| 4. Test case alignment | PASS | AC-09 seam genuine (disjoint non-empty sets, seed-via-runtime / read-via-resolver); AC-10 cross-slug, AC-13 live-PID, audit refusal + force-no-bypass, AC-02/03/04/05/07 covered. |
| 5. Code quality | PASS (2 WARN) | Compiles; no stubs / no non-test `.unwrap()`. WARNs: stale `#[allow(dead_code)]`; pre-existing files >500 lines (not this feature). |
| 6. Security | PASS | Validation at CLI edge before any FS/DB; traversal closed structurally (`ProjectSlug` newtype); no secrets; no injection; no new deps. |
| 7. Knowledge stewardship | PASS | All 5 dev agent reports carry `## Knowledge Stewardship` with `Queried:` + `Stored:`/"nothing novel — {reason}". |

## Detailed Findings

### 1. Pseudocode fidelity — PASS
`projects/slug_store.rs:63-113` implements `resolve_slug_store` in the exact ordered contract from `pseudocode/resolve_slug_store.md`: validate (step 1) → base via `data_dir.parent().map(...).unwrap_or_else(|| data_dir.clone())` (step 2) → `per_slug_data_dir(&base, &slug)` (step 3) → `db_path`/`vector_dir` from `PROJECT_DB_NAME`/`PROJECT_VECTOR_DIR` constants (step 4) → `db_path.exists()` gate returning `ServerError::Config` naming the absolute path (step 5).
- Export (`export.rs:99-102`): `Some(raw) => resolve_slug_store(&paths, raw)?.db_path`, `None => paths.db_path.clone()` — matches `export.md`.
- Import (`import/mod.rs:145-153`): resolves `(db_target, vector_target)` from the funnel; PID stays `paths.pid_path` — matches `import.md`.
- AC-06 summary (`export.rs:249-256`): `entries = total − skip_entries` with a comment citing pattern #5707 (report WRITTEN, not skipped). `test_do_export_written_count_excludes_skipped` proves it.

### 2. Architecture compliance — PASS (hard invariants verified in committed code)
- **One base derivation, no unwrap**: `slug_store.rs:78-82` — `unwrap_or_else`, no `.unwrap()/.expect()`.
- **One join site**: `per_slug_data_dir` is `pub(crate)` (`projects.rs:123`), takes `&ProjectSlug`; slug-mode calls come only from the funnel.
- **One validation edge**: `ProjectRegistry::validate_slug` `pub(crate)` (`projects.rs:206`), called once in the funnel before any FS/DB.
- **Existence gate strictly before open (C-3)**: gate at `slug_store.rs:97`; `SqlxStore::open` reached only after `?` returns Ok (export.rs:111, import/mod.rs:165).
- **PROJECT_VECTOR_DIR reused, no second literal**: consts pre-exist in `projects.rs:53-55`; slug_store imports via `super::` (`slug_store.rs:28`). Feature diff to projects.rs is only +6 lines (mod decl + 2 visibility raises) — did NOT add a new literal. The one bare `"vector"` in feature code is a test assertion guarding against literal drift.
- **Import live-PID-only refusal before any write (ADR-003)**: `preflight_live_pid_refusal` (`import/mod.rs:357-372`) uses liveness (`read_pid_file` + `is_process_alive` + `is_unimatrix_process`), reads base-scoped `paths.pid_path` (line 160), placed pre-open; no `--force` override.
- **Non-empty-audit pre-flight refusal, unbypassable by --force (ADR-005/C-5)**: `check_preflight` (`import/mod.rs:320-333`) runs at Phase 3, before `drop_all_data` (Phase 5, line 220); names `db_target`; test proves `--force` still refuses and the raw SQLite UNIQUE is never surfaced.
- **Vector rebuild into slug_dir/vector (ADR-004)**: `reconstruct_embeddings(&store, &vector_target)` (`import/mod.rs:263`).
- **No-slug byte-for-byte**: `None` arm returns `paths.db_path`/`paths.vector_dir` unchanged; AC-05 parity tests present.

### 3. Interface implementation — PASS
`SlugStorePaths` and `resolve_slug_store` signatures match ARCHITECTURE Integration Surface. `run_export`/`run_export_with_base`/`run_import`/`run_import_with_base` gained `slug: Option<&str>`; `main.rs:580-604` forwards `slug.as_deref()`. Clap `--slug` fields (`main.rs:342,372`) carry FR-15/AC-07 help wording.

### 4. Test case alignment — PASS
- **AC-09 seam (non-ceremonial)** `export.rs:2797`: seeds set_a=[101,102,103] via runtime literal-slug layout (`seed_slug_store` → `per_slug_data_dir`) and set_b=[201,202,203] (disjoint, non-empty) via path-hash `paths.db_path`; reads via `run_export_with_base(..., Some("alpha"), ...)`; asserts emitted == A and emitted ∩ B == ∅. Paired divergence guard proves no-slug emits B.
- **AC-10 cross-slug round-trip** `import_integration.rs:2000`: A→B into a fresh audit-empty slug, all tables restored, f64 confidence bit-exact, A untouched, vector into B/vector and nothing into path-hash vector.
- **AC-13 live-PID** `import_integration.rs:1753`: spawns a real process named `unimatrix`, asserts both predicates, refusal names PID path + remedy, no vector written; stale-PID test proves non-block.
- **Audit refusal** `import_integration.rs:1851`: refuses, names slug db path + "register", no UNIQUE leak, `--force` no bypass.
- AC-03 missing-store (fs unchanged), AC-04 validation/traversal (zero fs touch), AC-07 help contract, AC-05 parity, AC-02 vector redirect — all present.

### 5. Code quality — PASS (2 WARN)
- **Compiles**: `cargo clippy -p unimatrix-server --tests` exit 0 (compiles all test targets).
- **Clippy**: feature files clean. Only 2 warnings, both in `mcp/response/verbosity.rs:192,208` (`repeat().take()`→`repeat_n`) — the KNOWN pre-existing vnc-044 #920 toolchain issue, not in this diff (per spawn prompt; full triage is 3c).
- **Stubs**: none in feature code. Two `TODO(W2-4)` in `main.rs:1060,1835` are PRE-EXISTING (commit 31e7d100d, 2026-03-19); not in the feature diff — not attributable to vnc-048.
- **Non-test `.unwrap()/.expect()`**: none. Funnel uses `unwrap_or_else`; `write_header` uses `unwrap_or_default`.
- **WARN (file length)**: `projects.rs` is exactly 500 (at limit) and `slug_store.rs` (new) is 376 — both compliant; the funnel was extracted to a new module specifically to keep `projects.rs` ≤500. Pre-existing files the feature added to remain over 500 (`export.rs` 3023, `import/mod.rs` 2261, `main.rs` 2449, `main_tests.rs` 1448) — dominated by inline `#[cfg(test)]` modules, predate this feature, not a regression it introduced. Repo-wide debt, not a gate blocker for vnc-048.
- **WARN (stale annotation)**: `#[allow(dead_code)]` on `SlugStorePaths` and `resolve_slug_store` (`slug_store.rs:37,62`) is now stale — both are used by the Wave-B export/import callers. Harmless (an unfulfilled `allow` does not warn), but should be removed for hygiene.

### 6. Security — PASS
- Untrusted `--slug` validated at the single CLI edge before any FS/DB (`validate_slug`: charset `^[a-z0-9][a-z0-9-]{0,62}$` + reserved set).
- Path traversal closed structurally: `per_slug_data_dir` accepts only `&ProjectSlug`; `../x`, `..`, `/abs`, `a/b`, `a\b`, `%2e%2e`, embedded NUL all rejected with tests asserting zero filesystem side effects.
- No hardcoded secrets. No shell/command injection in product code (test-only spawn of a copied `sleep`). Import deserialization validates header, runs hash + chain validation, rolls back on parse/FK failure. No new dependencies added by this diff.

### 7. Knowledge stewardship — PASS
All five design/impl agent reports contain `## Knowledge Stewardship` with `Queried:` and `Stored:`/"nothing novel — {reason}":
- agent-3 (funnel): queried context_briefing; nothing-novel with reason.
- agent-4 (export): stored #5707 (skipped-vs-written count trap).
- agent-5 (import): stored #5708 (live-PID test fixture gotcha).
- agent-6 (main-dispatch): nothing-novel with reason.
- agent-7 (readme): nothing-novel with reason.

## Posted dev-agent test results (not re-run here; 3c owns the full suite)
- slug_store lib: 14/0; projects lib: 60/0.
- export lib: 82/0; import lib: 52/0; import_integration: 26/0.
- main-dispatch (final Wave C, closed C-9 call-site arity): full `unimatrix-server` crate PASS, 0 failed. Intermediate wave-B "does not compile until None arg added" notes are resolved by this wave; validator clippy `--tests` (exit 0) independently confirms the crate compiles.

## Rework Required
None.
