# Scope Risk Assessment: vnc-048

Per-slug backup/restore for personal-cloud. Root risk class: two resolvers over one base (#5507). Historical evidence woven in.

## User-Facing Entry Points & Behavioral Outcomes

| Entry point (how the operator actually invokes it) | Path-independent outcome they must observe |
|---|---|
| `unimatrix --project-dir <dir> export --slug <slug> -o <file>` | `<file>` contains the actual corpus of the slug the runtime writes to — not a sibling/hash store; stderr count summary reflects it |
| `unimatrix --project-dir <dir> import --slug <slug> -i <file>` | after the restart sequence, that named slug serves the restored corpus, vector search included |
| `export` / `import` with NO `--slug` | behavior byte-for-byte identical to today (single-project/local) |
| `export/import --slug <slug>` where no store exists at the resolved path | fails loud, creates nothing, names the fully resolved absolute path + next action |
| `export/import --slug <bad>` (charset-invalid / reserved) | rejected at CLI edge before any filesystem or DB touch |
| `register → stop → import --slug → start` sequence | restored slug serves vector search after `start`; the rebuilt index is the one loaded |
| `import --slug` while a daemon PID is live | hard-errors naming the PID path + `stop→import→start` remedy — never a partial/clobbered restore |

Outcomes are what the operator observes. No outcome is stated as an internal path; each must hold from the CLI invocation, not from a seam beneath it.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | **Two resolvers over one base.** CLI resolves path-hash; runtime writes literal-slug. They only disagree when a test seeds one and reads the other (#5507). A hash dir name is itself a charset-valid slug, so a wrong resolve returns a *real* store, not an error. | High | High | Single join site (`per_slug_data_dir`, `&ProjectSlug` only); AC-09 seam test must seed via runtime layout and read via CLI, proving disagreement — an N=1 same-path test is ceremonial (#4974). |
| SR-02 | **`SqlxStore::open` auto-creates + migrates (a write).** Any resolve that reaches `open` before an existence check re-stages the exact silent-empty-store bug in a new costume. | High | Med | Existence check strictly before `open` on both paths (C-3); never let `open` be the gate. |
| SR-03 | **Live-daemon vector clobber.** Import rebuilds `{slug}/vector` on disk, but a live daemon holds a stale boot-time index and overwrites it at shutdown (#5269, `shutdown.rs` dumps `per_slug_vectors`). DB rows survive; vector search silently reverts. | High | Med | Live-PID-only hard-error (OQ-1/C-4) makes the clobber structurally unreachable, not discipline-avoided. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | **File-existence vs registration gate.** Gating on `project list` (config-driven, #4972) would refuse the high-value export from a de-registered project pre-`delete --purge`. But "any store dir under base" also resolves stray/hash dirs. | Med | Med | Gate on file existence, not registration (per Approach); document in help that `--slug` = a store dir, not a registered project. |
| SR-05 | **Non-empty-`audit_log` restore target.** `drop_all_data` cannot clear append-only `audit_log`; explicit-`event_id` INSERT hits UNIQUE collision even with `--force` (C-5). Supported target is a *fresh* (audit-empty) slug only. | Med | Med | Pre-flight refuse on non-empty `audit_log` (OQ-2) with the "register a fresh slug" message — never a raw SQLite error. |
| SR-06 | **Scope-creep magnet: slug-awareness for the other 6 CLIs** (`verify`, `snapshot`, etc.) share the same gap. Pressure to fix all in one pass. | Low | Med | Hold the line — establish the `--slug` pattern here; whether the rest becomes one tracked item is a human call, not filed here. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | **Restore sequence is load-bearing and multi-step** (`register→stop→import→start`, #5344 ADR-004). If discoverable only via `project register` output, operators skip `stop` and hit SR-03. | High | Med | README is canonical (OQ-3); import `--slug` help points to it (AC-07/AC-12). |
| SR-08 | **Round-trip hash/chain validity.** Hash covers emitted rows, not source-DB rows (#4617); the `--skip-quarantined`/`audit_log` asymmetry means an audit-rows-only export is a *legitimate* output — the very thing that made the empty export look successful. | Med | Med | Fail-loud via stderr count summary (AC-06); "exported 0 entries" self-diagnoses. Do not touch the filter (Non-Goal). |

## Path-Divergence Risks

| Risk ID | Entry point | Divergence (path that works vs. path the operator invokes) | Recommendation |
|---|---|---|---|
| SR-09 | `export --slug` | Seam/unit test proves the resolver one layer down, while the CLI invocation still resolves the hash store and exports an audit-only file reporting success. | AC-09 must drive `run_export_with_base(slug=...)` and assert it emits the slug's rows and NONE of the hash store's — the resolvers must be able to disagree. |
| SR-10 | `import --slug` | DB rows land in the slug store (looks restored) while the served vector index is the daemon's stale one — restore "works" and does not. | Prove the outcome from `start` onward (AC-12): vector search served post-restart, not just rows present on disk. |
| SR-11 | any `--slug` on host, outside container | Host `$HOME` base resolves a different `.unimatrix` than the bind-mounted container base; accepts input, silently misses. | Fail loud with the resolved absolute path (C-7) — the resolved path is what distinguishes a base miss from a typo. Accepted-but-inert input must never no-op. |

## Assumptions

- **Base = `data_dir.parent()` holds in all four deploy shapes** (in-container `HOME=/data`, local dev, `*_with_base` test hook, host bind-mount) — SCOPE C-1. If any shape's `parent()` is not the `.unimatrix` base, every `--slug` resolve in that shape misses silently. This is the boundary-risk axis: each shape needs coverage or explicit fail-loud, not one representative shape. (SR-11 is the host-bind-mount corner of this axis.)
- **A freshly-registered slug store has a zero-row `audit_log`** — SCOPE Background Research / OQ-2. If register ever writes an audit row, SR-05's "supported target" collapses.
- **One daemon serves all slugs; its PID is base-scoped in the path-hash `data_dir`** — SCOPE Approach. The live-PID-only gate (SR-03) depends on this being the daemon's PID, not a per-slug one.

## Design Recommendations

1. **Architect:** one join site + one base derivation + one validation edge (SR-01, SR-02). Traversal closed structurally at `ProjectSlug::try_from` (C-2), not at runtime.
2. **Spec writer:** the four deploy shapes (C-1) are a coverage axis — require each to either resolve correctly or fail loud with the resolved path (SR-11, Assumptions).
3. **Spec writer:** every accept-but-inert path (missing store, non-empty audit_log, host base miss, live daemon) is a **fail-loud** requirement naming the resolved absolute path (SR-02/05/11, SR-03), never a silent no-op.
4. **Both:** the restore outcome is proven from `start`, not from disk state (SR-10); README-canonical sequence (SR-07).
