# Risk Coverage Report: vnc-048 (Per-Slug Backup/Restore — `--slug` on export/import)

Stage 3c execution. Branch `feature/vnc-048`. Primary functional coverage = Rust cargo tests
(`unimatrix-server`); infra-001 pytest smoke runs as a non-regression gate only (feature adds no
MCP-visible surface).

## Gate Non-Negotiables — VERDICT

| Gate | Requirement | Status | Evidence |
|------|-------------|--------|----------|
| **AC-09 / R-01 S1 — disagreement seam** | Seed slug store via runtime literal-slug layout (set A), seed path-hash store with disjoint NON-EMPTY set B, drive `run_export_with_base(slug=Some)`, assert `emitted == A ∧ ∩B == ∅`. N=1 same-path is ceremonial (#4974). | **PASS (genuine)** | `export::tests::test_export_slug_emits_slug_store_not_hash_store` |
| **AC-12 / R-03 S2 — served vector from `start`** | Full `register → stop → import --slug → start`, then a **served vector query** returns restored hits — proven from `start`, not disk state. Disk-state stat (AC-02) is necessary but does NOT discharge SR-10. | **PASS (genuine)** | `import_integration::test_restore_sequence_serves_vector_search_from_start` (commit `18c50cdb`) |

**BOTH gate non-negotiables (AC-09, AC-12) are genuine and PASS.**

**AC-09 is genuine and passes.** The seam test seeds set A `{101,102,103}` through the runtime layout
(`per_slug_data_dir(base,&ProjectSlug) + SqlxStore::open`) and set B `{201,202,203}` (non-empty,
disjoint) into the path-hash store via `ensure_data_directory().db_path` — two distinct code paths —
then drives the CLI resolver via `run_export_with_base(slug=Some("alpha"))` and asserts
`emitted == A` and every B id absent. The paired divergence guard
`test_export_no_slug_emits_hash_store_divergence_guard` proves no-slug emits B, so the fixture is not
aliasing one store onto both. This is NOT a ceremonial N=1 test.

**AC-12 is now satisfied — gate gap CLOSED (rework iteration 1, commit `18c50cdb`).** The added test
`test_restore_sequence_serves_vector_search_from_start` (import_integration.rs) drives the assembled
restore sequence (register A + seed a semantically-distinct 3-entry corpus → `run_export_with_base(slug=Some("aslug"))`
→ register fresh B → `run_import_with_base(slug=Some("bslug"), skip=false)`), then simulates `start`
by loading the POST-IMPORT `{bslug}/vector` through the **daemon's exact boot-time per-slug vector-load
path**: `SqlxStore::open(&b_db, PoolConfig::default())` wrapped in `Arc` → probe
`unimatrix-vector.meta` → `VectorIndex::load(Arc::clone(&store), VectorConfig::default(), &b_vector_dir)`.
Independently verified against `http_provision.rs:186-224` (`build_project_server`) — the calls match
verbatim; no pre-import in-memory index is constructed, so none can be reused. It then embeds a query
with the SAME `OnnxProvider`/`EmbedConfig::default()` model `reconstruct_embeddings` uses and asserts
the SERVED result: `boot_index.search(...)` returns `results[0].entry_id == 1` (the restored
async-runtime entry ranks top over sourdough/hiking distractors) and `boot_index.point_count() == 3`.
The only `file_count` call is a negative precondition guard (path-hash `vector/` untouched), NOT the
outcome. This is a genuine assembled-path proof, NOT a disk-state/in-memory proxy — it is exactly what
closes the #917/#918/#930 family for SR-10. (In-process library-crate boot equivalence rather than a
full HTTP daemon because `build_project_server` lives in the binary crate, unreachable from `tests/`;
the boot path's `SqlxStore::open` + `VectorIndex::load` pair is the narrowest faithful equivalent and
is what boot runs verbatim.)

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Resolver disagreement unproven (Critical) | `test_export_slug_emits_slug_store_not_hash_store`, `test_export_no_slug_emits_hash_store_divergence_guard` | PASS | **Full** |
| R-02 | `open` reached before existence gate (Critical) | `test_resolve_slug_store_missing_db_errors_before_open`, `test_export_slug_missing_store_fails_loud_fs_unchanged`, `test_import_slug_missing_store_fails_loud_fs_unchanged` | PASS | **Full** |
| R-03 | Live-daemon vector clobber / served-vector (Critical) | S1: `test_import_slug_live_pid_hard_errors_no_vector_write` PASS. S2 served-vector-from-`start`: `test_restore_sequence_serves_vector_search_from_start` PASS | PASS | **Full** |
| R-04 | Round-trip lossless A→B (High) | `test_import_slug_all_tables_into_fresh_slug_b_vector_redirect` (all tables + f64 bit-exact + chain via skip=false + A untouched) | PASS | **Full** |
| R-05 | Base-derivation per deploy shape (High) | `test_resolve_slug_store_base_is_data_dir_parent`, `_in_container_shape_derivation`, `_local_dev_shape_derivation`, `_parent_none_uses_fallback_no_unwrap` | PASS | **Full** |
| R-06 | Host bind-mount silent no-op (High) | `test_resolve_slug_store_host_base_miss_fails_loud_with_resolved_path` + integration fail-loud path | PASS | **Full** |
| R-07 | Non-empty-audit restore refusal (High) | `test_import_slug_nonempty_audit_refuses_preflight` (pre-write, names slug db + "register", no raw UNIQUE, `--force` cannot bypass) | PASS | **Full** |
| R-08 | Slug validation bypass / traversal (High) | `test_resolve_slug_store_rejects_charset_invalid`, `_rejects_traversal`, `_rejects_reserved`, `_accepts_boundary_lengths`; `test_export_slug_invalid_rejected_no_fs_touch`, `test_import_slug_invalid_rejected_no_fs_touch` | PASS | **Full** |
| R-09 | No-`--slug` fallthrough parity (High) | `test_export_no_slug_emits_hash_store_divergence_guard`, `test_import_no_slug_writes_to_path_hash_data_dir` + existing suites unchanged (21 export / 19 import pre-existing) | PASS | **Full** |
| R-10 | Silent sparse export (Med) | `test_format_export_summary_stdout_dest_sparse_self_diagnoses`, `test_format_export_summary_file_dest`, `test_do_export_returns_written_counts`, `test_do_export_written_count_excludes_skipped` | PASS | **Full (unit/format level)** |
| R-11 | Live-PID gate correctness (Med) | `test_import_slug_live_pid_hard_errors_no_vector_write`, `test_import_slug_stale_pid_does_not_block` | PASS | **Full** |
| R-12 | Restore sequence discoverability (Med) | README canonical sequence (register→stop→import→start, lines 85-91) + `test_cli_export_slug_help_states_contract`, `test_cli_import_slug_help_carries_readme_pointer` | PASS | **Full** |
| R-13 | `--slug` resolves stray/hash dir (Low→Med) | `test_export_no_slug_with_populated_slug_dir_emits_only_hash` | PASS | **Full** |
| R-14 | Partial-failure side effects (Med) | FS-unchanged assertions in export/import missing-store + invalid-slug no-fs-touch tests | PASS | **Full** |

## Test Results

### Unit Tests (`cargo test -p unimatrix-server --lib`)
- Total: 4563 · Passed: 4562 · Failed: 1 (pre-existing unrelated flake — see below)
- vnc-048 funnel units: `projects::slug_store::tests` — **12 passed** (base derivation ×4, validation edge ×4, existence gate ×2, host-base-miss, SlugStorePaths shape)
- vnc-048 export inline units (AC-09 seam, divergence guard, fail-loud, validation, hash-boundary, AC-06 summary): **all pass**
- The single failure is `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` — NOT vnc-048 code (eval-odometer harness, ass-098 branch base; empty vnc-048 diff). Flaky: failed 2/3, passed on re-run. Already tracked: **GH #790**.

### Integration Tests (Rust, `cargo test -p unimatrix-server --test ...`)
- `export_integration`: **22 passed, 0 failed** (21 prior + AC-08 `test_export_slug_readonly_under_wal_writer`, added pre-merge polish `ad214b6c`)
- `import_integration`: **27 passed, 0 failed** (8 new slug tests + 19 pre-existing) — re-run foreground post-rework, includes the added `test_restore_sequence_serves_vector_search_from_start`
- New slug integration tests (all PASS): `test_import_slug_live_pid_hard_errors_no_vector_write`, `test_import_slug_stale_pid_does_not_block`, `test_import_slug_nonempty_audit_refuses_preflight`, `test_import_slug_all_tables_into_fresh_slug_b_vector_redirect`, `test_import_slug_missing_store_fails_loud_fs_unchanged`, `test_import_slug_invalid_rejected_no_fs_touch`, `test_import_no_slug_writes_to_path_hash_data_dir`, `test_restore_sequence_serves_vector_search_from_start` (AC-12 gate, rework iter 1)

### Integration Smoke Gate (infra-001 pytest, non-regression)
- `pytest suites/ -v -m smoke --timeout=60`: **35 passed, 667 deselected, 0 failed** (268s)
- **Rework re-verification (carry-forward):** the only change since this run is one ADDED Rust test in `import_integration.rs` (commit `18c50cdb`); no MCP tool surface, no server source, no `pub(crate)`/signature change was touched. Smoke result carried forward — no re-run required.
- Confirms the `pub(crate)` visibility raises + `run_export`/`run_import` signature changes leaked nothing onto the MCP tool surface. Tool-count assertion (15, per #942) green via `test_tools`.
- No new pytest tests added — Stage 3a determined this feature is CLI-only with no MCP-visible surface (correct; smoke is a pure non-regression guard).

### Other Workspace Crates
- `unimatrix-vector --lib` in isolation: **113 passed, 0 failed**. One parallel-load flake under full `--workspace`: `index::tests::test_self_search_50_entries` (HNSW ANN-recall, exact-rank assertion under CPU contention). NOT vnc-048 code (empty vnc-048 diff in `unimatrix-vector`). Passes 3/3 targeted + 113/113 isolation. Filed **GH #958**.

### Static Analysis / Build Gates
- `cargo clippy -p unimatrix-server -- -D warnings`: **clean (rc=0)**
- `cargo clippy --workspace -- -D warnings`: **clean (rc=0)** — the pre-existing #935 `verbosity.rs manual_repeat_n` lint did NOT surface on this toolchain; workspace clippy is the only blocker channel and it is green. No new xfail/issue needed.
- Full-workspace LINK smoke (#878 guard, `check-workspace-link-smoke.sh`): **PASS** — profile invariant holds, all workspace test binaries link at configured parallelism.
- `cargo build --release`: **clean (rc=0)**.

## Gaps

1. **AC-12 / R-03 S2 — served vector search from `start` (GATE NON-NEGOTIABLE): CLOSED (rework iter 1).**
   `test_restore_sequence_serves_vector_search_from_start` (commit `18c50cdb`) now drives the assembled
   restore sequence and asserts a served vector query returns the restored corpus via the daemon's exact
   boot-time load path (`SqlxStore::open` + `VectorIndex::load` against POST-IMPORT `{slug}/vector`,
   verified against `http_provision.rs:186-224`). Independently confirmed genuine — an assembled-path
   proof, not the prior disk-state/in-memory proxy. **No remaining gate gap.**

2. **AC-08 (Med, non-gate) — export against a live daemon's slug store read-only: CLOSED (pre-merge
   polish, commit `ad214b6c`).** `test_export_slug_readonly_under_wal_writer` (export_integration.rs)
   now covers it and passes. No remaining gap. See Acceptance Criteria row AC-08 and the pre-merge
   re-verification section below.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_export_slug_emits_slug_store_not_hash_store` (emitted == slug store's corpus) + `test_resolve_slug_store_base_is_data_dir_parent` |
| AC-02 | PASS (disk-state) | `test_import_slug_all_tables_into_fresh_slug_b_vector_redirect` — fresh HNSW under `{slug}/vector`, nothing under path-hash `vector/` |
| AC-03 | PASS | `test_export_slug_missing_store_fails_loud_fs_unchanged`, `test_import_slug_missing_store_fails_loud_fs_unchanged`, `test_resolve_slug_store_missing_db_errors_before_open` — non-zero/`Err`, absolute path named, FS unchanged |
| AC-04 | PASS | `test_export_slug_invalid_rejected_no_fs_touch`, `test_import_slug_invalid_rejected_no_fs_touch` + unit `_rejects_charset_invalid`/`_rejects_traversal`/`_rejects_reserved` |
| AC-05 | PASS | `test_export_no_slug_emits_hash_store_divergence_guard`, `test_import_no_slug_writes_to_path_hash_data_dir` + existing suites unchanged (stderr excluded per WARN-1; no existing test asserts stderr emptiness) |
| AC-06 | PASS (unit/format) | `test_format_export_summary_file_dest`, `test_format_export_summary_stdout_dest_sparse_self_diagnoses`, `test_do_export_returns_written_counts` |
| AC-07 | PASS | `test_cli_export_slug_help_states_contract`, `test_cli_import_slug_help_carries_readme_pointer` |
| AC-08 | **PASS (COVERED)** | `test_export_slug_readonly_under_wal_writer` (export_integration.rs, commit `ad214b6c`) — seeds the slug store via the runtime literal-slug layout, holds a **second live `SqlxStore`/pool** open in WAL + `busy_timeout` (the same per-connection config `build_project_server` holds each per-slug store at boot, `pool_config.rs:143-148`), spawns a **background thread doing continuous INSERTs** through a cloned `write_pool_server()` handle (AtomicBool-gated), then asserts `run_export_with_base(slug=Some("livedaemon"))` **succeeds** (no lock error) and emits the seeded corpus (ids 101/102/103). Genuine read-under-live-writer coexistence, not an export against a closed/idle store; narrowest faithful daemon-free equivalent |
| **AC-09** | **PASS (gate, genuine)** | `test_export_slug_emits_slug_store_not_hash_store` — set A via runtime layout, disjoint non-empty B via path-hash, `emitted == A ∧ ∩B == ∅`; divergence guard confirms |
| **AC-10** | **PASS (gate)** | `test_import_slug_all_tables_into_fresh_slug_b_vector_redirect` — all tables A→B (two distinct slugs), f64 bit-exact, chain via `skip=false`, A untouched; `test_import_slug_nonempty_audit_refuses_preflight` retires the audit-collision half |
| AC-11 | PASS | `test_export_no_slug_with_populated_slug_dir_emits_only_hash` |
| AC-12 | **PASS (gate, genuine)** | README file-check (canonical sequence, README:85-91) + `test_restore_sequence_serves_vector_search_from_start` — assembled `register→export→register→import` then served vector query via the daemon boot path (`SqlxStore::open` + `VectorIndex::load`), asserts restored async-runtime entry ranks top (`entry_id==1`, `point_count==3`); not a disk-state proxy (commit `18c50cdb`) |
| AC-13 | PASS | `test_import_slug_live_pid_hard_errors_no_vector_write` (names PID path + remedy, no `{slug}/vector` write), `test_import_slug_stale_pid_does_not_block` |

## Pre-Existing / Unrelated Failures (triaged, NOT vnc-048)

| Signal | Test | Triage | Reference |
|--------|------|--------|-----------|
| Flaky (2/3 fail) | `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` | Pre-existing on ass-098 branch base; empty vnc-048 diff; passes on re-run | **GH #790** (already open) |
| Flaky (1/4 fail, parallel-load only) | `unimatrix-vector index::tests::test_self_search_50_entries` | Pre-existing ANN-recall flake; empty vnc-048 diff; 113/113 isolation | **GH #958** (filed this session) |
| Not reproduced | `verbosity.rs manual_repeat_n` (#935) | Did not surface — workspace clippy clean on this toolchain | GH #935 (referenced, no action) |

No integration tests were deleted, commented out, or `xfail`-marked. The two Rust flakes are cargo
unit tests (not infra-001 pytest), so the pytest `xfail` workflow does not apply; both are tracked by
GH issue instead.

## Rework Re-Verification (Stage 3c iteration 1 — AC-12 close)

Targeted re-run after import-dev rework commit `18c50cdb` (one added test; no source/tool-surface change):
- `cargo test -p unimatrix-server --test import_integration` → **27 passed, 0 failed** (foreground), incl. `test_restore_sequence_serves_vector_search_from_start`.
- New test independently read + verified genuine: loads POST-IMPORT `{slug}/vector` via the daemon boot path (`SqlxStore::open` + `VectorIndex::load`, matching `http_provision.rs:186-224` verbatim), issues a real ONNX-embedded served query, asserts semantic ranking — NOT a disk-state/in-memory proxy.
- `cargo clippy -p unimatrix-server -- -D warnings` → **clean (rc=0)**. (`--tests` surfaces only the pre-existing #935 `verbosity.rs manual_repeat_n` lint — unrelated file, not vnc-048.)
- Smoke gate + full workspace unit run (4562 pass), export 21/21, LINK smoke, `cargo build --release`: **carried forward** from the prior full run — unchanged surface. Rust flakes #790 / #958 unchanged (still tracked, not vnc-048).
- **Both gate non-negotiables (AC-09, AC-12) PASS.** AC-08 (Med, non-gate) read-only-export-under-WAL is now CLOSED (see below).

## Pre-Merge Polish Re-Verification (Stage 3c iteration 2 — commit `ad214b6c`)

Targeted re-run after the pre-merge polish commit (non-gating, human-requested). Two source changes in
`unimatrix-server`: (1) added the AC-08 export test + a `seed_slug_store` helper in
`export_integration.rs`; (2) removed two stale `#[allow(dead_code)]` and the now-unused `slug_dir` field
from `SlugStorePaths` in `projects/slug_store.rs`.

- **`SlugStorePaths` is now a 2-field struct** (`db_path`, `vector_dir`) — the `slug_dir` field was
  removed as genuinely dead (only `db_path`/`vector_dir` are consumed by the export/import callers;
  `slug_dir` was read only by the struct's own `#[cfg(test)]` unit tests, which were adapted to derive
  the slug dir from `db_path`'s parent). This differs from the pseudocode's 3-field draft; base-derivation
  correctness is still fully proven via the adapted unit tests.
- **Struct-change blast radius (independently re-verified):** grep for `.slug_dir`/`slug_dir:` field
  access across `crates/` returns no production reference to the removed field — the only `.slug_dir(...)`
  hits are a test-fixture method on `projects/tests.rs`'s fixture (`fx.slug_dir(slug)`), unrelated to
  `SlugStorePaths`. Nothing else referenced the removed field.
- `cargo build -p unimatrix-server --tests` → **0 errors** (whole crate compiles after the field removal).
- `cargo test -p unimatrix-server --test export_integration` → **22 passed, 0 failed** (incl.
  `test_export_slug_readonly_under_wal_writer`), foreground.
- `cargo test -p unimatrix-server --test import_integration` → **27 passed, 0 failed** (unchanged — the
  funnel struct change did not break import's `vector_dir` consumption), foreground.
- `cargo test -p unimatrix-server --lib projects::slug_store` → **12 passed, 0 failed** (the three adapted
  unit-test assertions pass; SlugStorePaths shape still proven via `db_path`).
- **AC-08 test independently confirmed GENUINE** (read line-by-line, `export_integration.rs:1588-1682`):
  seeds the per-slug store via the runtime literal-slug layout; holds a **second live `SqlxStore`/pool**
  open on the same slug db across the export in WAL + `busy_timeout` (byte-for-byte the daemon's boot-time
  per-slug handle, `pool_config.rs:143-148`); spawns a **background thread performing continuous INSERTs**
  through a cloned `write_pool_server()` handle (AtomicBool-gated), so the export reads *alongside* an
  active writer — NOT an idle handle and NOT a closed/idle store; then asserts `run_export_with_base(
  slug=Some("livedaemon"))` **succeeds** and emits the seeded corpus (101/102/103). This is the real
  assembled CLI resolver path against a live-writer store — a faithful proxy for AC-08's "live daemon's
  slug store", not a proxy against a quiescent store.
- `cargo clippy -p unimatrix-server -- -D warnings` → **clean (rc=0)** (feature files clean). `--tests`
  surfaces only the pre-existing #935 `verbosity.rs manual_repeat_n` lint — an untouched, non-vnc-048 file,
  tracked separately; NOT a vnc-048 issue.
- **Gate non-negotiables AC-09 and AC-12 UNAFFECTED** — neither was touched by this polish; their genuine
  PASS verdicts carry forward unchanged.
- Smoke gate, full-workspace unit run, LINK smoke, `cargo build --release`: **carried forward** from the
  prior full run — no MCP tool surface, server source signature, or `pub(crate)` visibility was changed by
  this polish (source deltas are one test file + a dead-field removal). Rust flakes #790 / #958 unchanged.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (task: vnc-048 Stage 3c execution) — surfaced #4781
  (Stage-3c pre-existing-failure triage procedure), #2758 (grep gate-report non-negotiable test names
  before accepting PASS), #4202/#4473/#4515/#3806 (Gate 3b/3c "named test never implemented" failure
  family). Directly applicable: #2758 and the #4202 family drove the AC-12 verification that caught the
  missing served-vector-from-`start` test behind a disk-state proxy.
- Stored: nothing novel — the operative lessons (disk-state-proxy vs assembled-path #917/#918/#930;
  named-test-never-implemented #4202) already exist in Unimatrix; this feature's AC-12 gap is a
  textbook instance, not a new pattern. The two pre-existing flakes are code defects → GH issues
  (#790 existing, #958 filed), per "bugs are GH issues, not lessons."
