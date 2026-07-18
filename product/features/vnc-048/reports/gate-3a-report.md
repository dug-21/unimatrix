# Gate 3a Report: vnc-048

> Gate: 3a (Component Design Review)
> Date: 2026-07-18
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | Single `resolve_slug_store` funnel holds the reuse triad + existence gate; ADR-001..006 honored in pseudocode. One WARN: crate mislabel. |
| Specification coverage | PASS | FR-1..16, NFR-1..9, AC-01..13 all have pseudocode + test-plan coverage; no scope additions. |
| Risk coverage | PASS | R-01..R-14 and SR-01..SR-11 each map to at least one test scenario; both gate non-negotiables are real, not ceremonial. |
| Interface consistency | PASS | `SlugStorePaths`, `resolve_slug_store`, and all four `run_*` signatures consistent across OVERVIEW / component / dispatch files. |
| Knowledge stewardship compliance | PASS | All four design agents (architect, risk, pseudocode, testplan) carry a `## Knowledge Stewardship` block with Queried + Stored/Declined-with-reason. |

## Hard-Invariant Verification (spawn-prompt checklist)

| Invariant | Status | Evidence |
|-----------|--------|----------|
| ONE base derivation, `data_dir.parent()`, no `unwrap` | PASS | `resolve_slug_store.md` Step 2: `.parent().map(Path::to_path_buf).unwrap_or_else(\|\| paths.data_dir.clone())`, Component 1 only; export/import re-derive nothing. |
| ONE join site (`per_slug_data_dir` behind `&ProjectSlug`) | PASS | `resolve_slug_store.md` Step 3 is the sole join; `&ProjectSlug` only; structural guard noted (a `&str` won't compile into the join). |
| ONE validation edge | PASS | `validate_slug` called only in the funnel, before any FS/DB (AC-04). |
| Existence check STRICTLY before `SqlxStore::open` (C-3) | PASS | Funnel Step 5 existence gate; export opens only after funnel `Ok`; import opens `db_target` only after funnel + live-PID gate (import.md gate-ordering table). |
| No second "vector" literal (reuse `PROJECT_VECTOR_DIR`) | PASS | Pseudocode uses the constant and explicitly flags the "second literal" trap. Confirmed against code: `PROJECT_VECTOR_DIR` already exists at `unimatrix-server/src/projects.rs:55` — OQ-1 resolved in fact (reuse the existing const). |
| Live-PID-only import refusal (ADR-003) | PASS | `preflight_live_pid_refusal` uses `read_pid_file`+`is_process_alive`+`is_unimatrix_process`; `[[projects]]` half explicitly dropped (R-11 S2). |
| Non-empty-audit pre-flight before any write (ADR-005) | PASS | `check_preflight` audit gate fires pre-`drop_all_data`/insert; never surfaces raw SQLite UNIQUE. |
| Vector rebuild into `slug_dir/vector`, PID base-scoped | PASS | import.md Phase 10 `reconstruct_embeddings(&store, &vector_target)`; PID stays `paths.pid_path` (two path sources explicitly "not tidied into one"). |
| Every accept-but-inert path fails loud naming resolved absolute path | PASS | Missing store → absolute `db_path.display()`; live-PID → `pid_path.display()`; audit gate → resolved slug db path (OQ-4 mandates it); charset/reserved → raw slug pre-FS. Full inventory in OVERVIEW.md fail-loud table. |
| AC-09/R-01-S1 disagreement seam is real, not ceremonial | PASS | `test_export_slug_emits_slug_store_not_hash_store`: A seeded via runtime literal-slug `seed_slug_store` (distinct code), B disjoint NON-EMPTY in hash store, read via `run_export_with_base(slug=Some)`; asserts `emitted==A ∧ ∩B==∅`; paired `slug=None`→B divergence guard; #4974 N=1-ceremonial comment mandated in the test file. |
| AC-12/R-03-S2 served vector search proven from `start` | PASS | `test_restore_sequence_serves_vector_search_from_start` drives the compiled binary through `register→stop→import→start`, then a served vector query; "proven from `start` onward, not disk state" stated; AC-02 disk stat marked necessary-but-not-sufficient. |
| AC-05 parity scoped to file/stdout/exit-code (stderr excluded, WARN-1) + stderr-emptiness check carried | PASS | export.md R-09 scopes identity to "exported file + stdout + exit code — stderr excluded" and carries the grep-for-stderr-assertion WARN-1 check; mirrored in import.md and OVERVIEW WARN-1 reconciliation. |

## Detailed Findings

### 1. Architecture alignment
**Status**: PASS
**Evidence**: The pseudocode reproduces the architecture's core contract exactly — `resolve_slug_store` performs validate → derive-base → join → existence-gate in one ordered place (`resolve_slug_store.md` Step 1-6), matching ARCHITECTURE.md §"Slug-mode resolution (single funnel)". ADR mapping is 1:1: ADR-001 (reuse triad) in the funnel; ADR-002 (pre-open existence gate) Step 5; ADR-003 (live-PID-only) `preflight_live_pid_refusal`; ADR-004 (vector into `slug_dir/vector`, PID base-scoped) import Phase 10 + explicit two-path-source note; ADR-005 (non-empty-audit pre-flight) `check_preflight`; ADR-006 (fail-loud resolved path + export stderr summary) funnel Step 5 + `emit_export_summary`. Both commands stay sync pre-tokio; import keeps the multi-thread runtime (C-8/GH#554) — carried in import.md and the test-plan conventions.

**WARN (crate mislabel)**: Pseudocode Component 1 header (`OVERVIEW.md`, `resolve_slug_store.md`) and SPECIFICATION Dependencies label `projects.rs` / `per_slug_data_dir` / `validate_slug` as `unimatrix-engine`. They actually live in `crates/unimatrix-server/src/projects.rs` (`per_slug_data_dir` at `:123`, `validate_slug` at `:206`, `PROJECT_VECTOR_DIR` at `:55`) — all cited line numbers match `unimatrix-server`. The design's `pub(crate)` visibility-raise strategy is only valid because the funnel, export.rs, import/mod.rs, and these two symbols are all same-crate (`unimatrix-server`); this internally confirms the true crate and makes the label self-correcting. `ensure_data_directory`/`ProjectPaths` are correctly attributed to `unimatrix-engine/src/project.rs`. Non-blocking; flagged so the 3b implementer places the funnel in `unimatrix-server/src/projects.rs`.

### 2. Specification coverage
**Status**: PASS
**Evidence**: Every FR maps to pseudocode: FR-1/2/4 funnel base+join; FR-3 validate_slug edge; FR-5/6 existence gate + fail-loud; FR-7 export slug branch; FR-8 `emit_export_summary` (both modes, stderr); FR-9 AC-08 read-only; FR-10 vector redirect; FR-11 PID base-scoped; FR-12 live-PID hard-error; FR-13 non-empty-audit refusal; FR-14 no import summary (honored — none added); FR-15 clap help text; FR-16 README section. NFR-1..9 addressed (parity property, fail-loud, four-shape axis, no-unwrap, sync-pre-tokio, no-new-base, blast-radius touching only `main.rs:556-567`+two test files, structural traversal closure, no filter change). No scope additions — pseudocode implements nothing outside the spec (skip-quarantined asymmetry explicitly untouched; no `--force` override; no import summary).

### 3. Risk coverage
**Status**: PASS
**Evidence**: OVERVIEW.md test-plan Risk→Test→AC table maps all R-01..R-14 to a plan location + vehicle. Critical risks: R-01 export.md seam (top weight) + divergence guard; R-02 FS-unchanged integration + unit ordering (`open` never the gate); R-03 CLI-sequence served query + structural-unreachability (`no write to {slug}/vector`). High/Med risks each carry scenarios (round-trip all-tables A→B with type fidelity + `chain_verify`; four-shape derivation axis incl. host-bind-mount fail-loud; non-empty-audit refusal + no-`--force`-bypass; parameterized traversal/reserved reject with zero FS side effects; no-slug parity property; sparse-export stderr; live-only PID predicate with stale/stanza-do-not-block; README/help discoverability; stray-hash-dir boundary; partial-failure no-side-effects). SR-01..SR-11 traced via the Scope Risk Traceability table. Both gate non-negotiables are concretely specified and explicitly reject the ceremonial/disk-state substitutes.

### 4. Interface consistency
**Status**: PASS
**Evidence**: `SlugStorePaths { slug_dir, db_path, vector_dir }` identical in OVERVIEW.md and resolve_slug_store.md; export.md consumes `.db_path`, import.md consumes `.db_path` + `.vector_dir` — no field drift. Signature order consistent: export `run_export(project_dir, output, slug, skip_quarantined, confirm)` (export.md) matches the dispatch in main-dispatch.md; import `run_import(project_dir, input, slug, skip_hash_validation, force)` matches its dispatch. `resolve_slug_store`/`per_slug_data_dir`/`validate_slug`/`reconstruct_embeddings`/pidfile primitives all trace to the ARCHITECTURE Integration Surface — no invented names. The shared `seed_slug_store` test helper has a single fixed signature pinned to `per_slug_data_dir(base:&Path, slug:&ProjectSlug)` and is reused across export/import test files (test infra cumulative; col-030 drift guard noted).

### 5. Knowledge stewardship compliance
**Status**: PASS
**Evidence**: architect (`agent-1-architect`) — Queried + `Stored: #5693–#5698` (ADR-001..006). risk (`agent-3-risk`) — Queried + `Declined: nothing novel to store -- {reason}`. pseudocode (`agent-1-pseudocode`, read-only) — Queried entries present + Declined-with-reason. testplan (`agent-2-testplan`, read-only) — Queried entries present + Declined-with-reason. All blocks present with reasons; no WARN.

## Non-blocking notes for the 3b/3c implementers (not FAILs)

- Place the funnel and the two `pub(crate)` raises in `unimatrix-server/src/projects.rs` (not `unimatrix-engine`); reuse the existing `PROJECT_VECTOR_DIR` at `projects.rs:55` — do not introduce a second `"vector"` literal (pseudocode OQ-1 is resolved by this fact).
- Pseudocode OQ-2 (carry `ExportCounts` out of `block_export_sync` vs. print inside the async block) and OQ-3 (output-path vs source-db path in the summary) are implementation choices; FR-8 wording ("resolved output path") governs OQ-3.
- Pseudocode OQ-4: the non-empty-audit message MUST name the resolved slug db path (`db_target`), not the path-hash path — carry `db_target` into `check_preflight` or move the audit gate after open where `db_target` is in scope.

## Rework Required

None — Gate 3a PASSES.

## Knowledge Stewardship
- Stored: nothing novel to store -- the operative gate patterns (ceremonial-seam-unless-value #4974, two-resolver-disagreement #5507, crate-attribution-vs-line-number discipline) already exist as knowledge or are feature-specific; the crate-mislabel finding is a one-off doc error, not a recurring cross-feature gate failure.
