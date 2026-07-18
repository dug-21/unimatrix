# Agent Report — vnc-048-agent-4-export (Component 2: export `--slug` branch + AC-06 summary)

## Summary
Threaded `slug: Option<&str>` through `run_export` / `run_export_with_base` /
`run_export_inner`; branched slug mode to the Wave-1 `resolve_slug_store` funnel (no
re-derive / re-join / re-validate; no second existence check or second `open`-as-gate);
added the AC-06 stderr count summary emitted after COMMIT on export success only.

## 1. Files modified
- `crates/unimatrix-server/src/export.rs` — slug branch, `ExportCounts`,
  `format_export_summary` / `emit_export_summary`, `do_export` returns counts,
  `export_audit_log` returns written-row count, 8 new unit tests + 2 helpers.
- `crates/unimatrix-server/src/main.rs` — export call site passes `slug = None`
  (Component 4 / Wave C wires the clap `--slug` flag later). My export hunk only;
  main.rs is co-edited by the import agent (separate, non-overlapping import hunk).

## 2. Exact `run_export*` signatures after threading slug
```rust
pub fn run_export(
    project_dir: Option<&Path>,
    output: Option<&Path>,
    slug: Option<&str>,
    skip_quarantined: bool,
    confirm: bool,
) -> Result<(), Box<dyn std::error::Error>>

pub fn run_export_with_base(
    project_dir: Option<&Path>,
    output: Option<&Path>,
    base_dir: &Path,
    slug: Option<&str>,
    skip_quarantined: bool,
    confirm: bool,
) -> Result<(), Box<dyn std::error::Error>>

fn run_export_inner(
    project_dir: Option<&Path>,
    output: Option<&Path>,
    base_dir: Option<&Path>,
    slug: Option<&str>,
    skip_quarantined: bool,
    confirm: bool,
) -> Result<(), Box<dyn std::error::Error>>
```
Slug placed right after `output` (before `skip_quarantined`), per pseudocode.
Store selection: `match slug { Some(raw) => resolve_slug_store(&paths, raw)?.db_path,
None => paths.db_path.clone() }` — the `?` fails loud BEFORE any `SqlxStore::open`.
Summary emitted inside the `block_export_sync` async block after COMMIT (OQ-2 option b:
`block_export_sync` signature untouched).

## 3. Tests — pass/fail
- `cargo test -p unimatrix-server --lib export` → **82 passed, 0 failed**.
- `cargo build -p unimatrix-server` → clean. `cargo clippy -p unimatrix-server --lib --bins -- -D warnings` → clean.
- New tests (all pass):
  - `test_export_slug_emits_slug_store_not_hash_store` — R-01 S1 seam (TOP weight):
    set A via runtime `http_provision` literal-slug layout at `{base}/<slug>/unimatrix.db`,
    disjoint non-empty set B in hash store; asserts emitted == A and emitted ∩ B == ∅.
    Carries the mandated #4974/#5507 "N=1 same-path is ceremonial" comment; seed path
    (per_slug_data_dir + direct open) is distinct code from the CLI read path.
  - `test_export_no_slug_emits_hash_store_divergence_guard` — R-01 S2: `slug=None` emits B.
  - `test_export_slug_missing_store_fails_loud_fs_unchanged` — AC-03/R-02: existence gate
    before open, error names the resolved absolute db path, no output/dir/-wal/-shm created.
  - `test_export_slug_invalid_rejected_no_fs_touch` — AC-04: charset/reserved/traversal
    rejected at the CLI edge, no output file.
  - `test_export_no_slug_with_populated_slug_dir_emits_only_hash` — AC-11: a 16-hex-looking
    populated slug dir is never reinterpreted in no-slug mode.
  - `test_format_export_summary_file_dest` / `_stdout_dest_sparse_self_diagnoses` — AC-06
    wording incl. `exported 0 entries` sparse self-diagnosis and `→ stdout`.
  - `test_do_export_returns_written_counts` / `_written_count_excludes_skipped` — count source.

## 4. Issues / notes (flags for the leader)
- **ADJACENT BREAKAGE (flag, not fixed): integration test call sites need the mechanical
  slug-arg threading (C-9).** My signature change to `run_export_with_base` breaks the
  existing `run_export_with_base` / `run_export` calls in
  `crates/unimatrix-server/tests/export_integration.rs` and `.../tests/import_integration.rs`
  (e.g. the `run_export_to_string` helper, line ~56). They still pass the old 5-arg form and
  will not COMPILE under a full `cargo test -p unimatrix-server` until a `None` slug arg is
  threaded. I did NOT touch them: my spawn prompt forbids modifying integration tests
  (Stage 3c), and both files are being concurrently edited by the import agent (Component 3)
  this wave — editing them would risk clobbering their uncommitted work (shared-worktree
  hazard). Route to the Stage 3c tester (owns export_integration.rs) / import agent.
- **AC-05 parity / WARN-1 (reconciled):** grepped `export_integration.rs` — NO existing test
  asserts on empty/absent stderr. The only stderr test (`test_skip_quarantined_stderr_reports_skip_counts`)
  merely asserts export succeeds, so the new summary line is additive, not a regression. No
  existing test edit needed for the summary.
- **Did NOT commit.** The import agent is concurrently editing overlapping files (main.rs,
  both integration tests); running git in the shared worktree now risks wiping their work.
  Leaving the wave commit to the leader.
- **Pre-existing: `export.rs` is 3023 lines (>500).** It was already 2645 before this change
  (~2200 of it is the `#[cfg(test)]` module). A split is out of scope this wave and would
  collide with concurrent edits; flagging for a future dedicated refactor.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced the export/import table-addition
  procedure (#4624) and ADR-008 skip-quarantined filter (#4615); nothing on the count-source
  gotcha I hit, so it was novel.
- Stored: entry #5707 "export.rs: export_entries returns SKIPPED count, not written — derive
  written from in-txn COUNT" via context_store (pattern, topic unimatrix-server). Captures the
  silent-wrong-count trap wiring the AC-06 summary, plus the `rustfmt --edition 2024` single-file
  fmt-churn-avoidance note.
