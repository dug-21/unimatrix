# Risk-Based Test Strategy: vnc-048

Per-slug backup/restore (`--slug` on `export`/`import`). Root risk class: **two resolvers over one base** (#5507). The design funnels both commands through `resolve_slug_store`; this strategy proves the funnel resolves the store the *runtime writes to* — from the operator's CLI invocation, not a seam beneath it.

Sources: SCOPE.md, SCOPE-RISK-ASSESSMENT.md (SR-01..SR-11), ARCHITECTURE.md (ADR-001..006), SPECIFICATION.md (FR-1..16, AC-01..13, C-1..10). Historical: #4974 (ceremonial-seam / N=1 false confidence), #4950 (single-funnel resolver), #5507 (two-resolver disagreement only shows when a test seeds one and reads the other), #5344 (ADR-004 single-restart two-slug), #2621 (open_readonly no-migrate).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | **Resolver disagreement unproven** — a same-path (N=1) seam test passes while the CLI still resolves the hash store; ships broken again (#4974/#5507). | High | High | **Critical** |
| R-02 | **`SqlxStore::open` auto-creates+migrates (a write)** reached before the existence gate → re-stages the silent-empty-store bug in a new costume (C-3). | High | Med | **Critical** |
| R-03 | **Live-daemon vector clobber** — DB rows land in the slug store but a live daemon's stale boot index overwrites the rebuilt one at shutdown; restore "works" and does not (SR-03/SR-10, ADR-003/004). | High | Med | **Critical** |
| R-04 | **Round-trip not lossless into a fresh slug** — a table/column/type drops silently across export→import into a *different* slug (AC-10). | High | Med | High |
| R-05 | **Base-derivation miss per deploy shape** — `data_dir.parent()` is not the `.unimatrix` base in some of the four shapes → every `--slug` resolve in that shape misses (C-1, NFR-3). | High | Med | High |
| R-06 | **Host bind-mount silent no-op** — host `$HOME` base ≠ container base; accepts input, misses (SR-11/C-7). | High | Med | High |
| R-07 | **Non-empty-`audit_log` restore surfaces raw SQLite UNIQUE** instead of the actionable "register a fresh slug" message (SR-05/C-5, ADR-005). | Med | Med | High |
| R-08 | **Slug validation bypass / path traversal** — a raw `&str` reaches `per_slug_data_dir`, or `..`/`%2e`/absolute path escapes the base (C-2, NFR-8). | High | Low | High |
| R-09 | **No-`--slug` fallthrough regression** — the added branch perturbs the path-hash path; single-project behavior no longer byte-for-byte identical (AC-05/NFR-1). | High | Low | High |
| R-10 | **Silent sparse export** — audit-rows-only (or 0-entry) export reports success with no signal; the original defect's tell (SR-08, AC-06). | Med | Med | Med |
| R-11 | **Live-PID gate false-negative/positive** — stale PID hard-errors a legit restore, or a live PID slips past and import proceeds (AC-13, ADR-003). | Med | Med | Med |
| R-12 | **Restore sequence undiscoverable** — README omission → operator skips `stop`, hits R-03 (SR-07, AC-12/16). | Med | Med | Med |
| R-13 | **`--slug` resolves a stray/hash dir** (file-existence gate, not registration) — a 16-hex hash dir is a charset-valid slug (SR-04, ADR-002/AC-11). | Low | Med | Med |
| R-14 | **Partial-failure side effects** — a failed import/export leaves dirs, a partial db, or an output file behind (AC-03 "creates nothing"). | Med | Low | Med |

## Risk-to-Scenario Mapping

### R-01: Resolver disagreement unproven (Critical) — AC-09, SR-01/SR-09
**Impact**: Feature ships green while `export --slug` still emits the near-empty hash store. This is the exact 2026 defect.
**Test Scenarios**:
1. **The disagreement seam (mandatory shape).** Using the `*_with_base` hook so `data_dir.parent() == X`: seed `X/<slug>/unimatrix.db` via the **`http_provision` literal-slug layout** with known entry set A; seed the path-hash store `X/<hash>/unimatrix.db` **differently** with disjoint entry set B. Call `run_export_with_base(project_dir, base=X, slug=Some("foo"))`. Assert emitted rows == A **and** intersection(emitted, B) == ∅.
2. Same seeding, no `--slug`: assert the export emits B (the hash store) — proving the two paths genuinely diverge and the fixture is not accidentally aliasing one store.
**Coverage Requirement**: The seed path (runtime literal-slug) and the read path (CLI resolver) MUST be different code, and set B MUST be non-empty and disjoint from A. **A same-path N=1 test (seed and read through the same `make_project_dir`/hash layout) is CEREMONIAL (#4974) and DOES NOT satisfy SR-01** — call it out in the test file; if B is empty or seeded through the same layout as A, the test proves nothing. This scenario carries top weight.

### R-02: `open` reached before existence gate (Critical) — AC-03, C-3
**Impact**: Auto-created empty store = the original bug re-costumed.
**Test Scenarios**:
1. `export --slug` and `import --slug` against a slug with **no** `unimatrix.db` at the resolved path → assert non-zero exit, error contains the fully-resolved **absolute** path, and **no** store/dir/WAL was created at that path (stat the parent before/after).
2. White-box/ordering assertion: existence check strictly precedes `SqlxStore::open` — a nonexistent slug never produces a migrated db file on disk.
**Coverage Requirement**: Filesystem is provably unchanged after the failure (no `unimatrix.db`, no `vector/`, no `-wal`/`-shm`, no output file). `open` is never the gate on either command.

### R-03: Live-daemon vector clobber (Critical) — AC-12/AC-13, SR-03/SR-10, ADR-003/004
**Impact**: Rows restored, vector search silently reverts to pre-restore index after next daemon shutdown.
**Test Scenarios**:
1. **Structural-unreachability proof**: with a live daemon PID present at the base-scoped `pid_path`, `import --slug` hard-errors (R-11/AC-13) — so the clobber path is never entered. Assert import does not write to `{slug}/vector`.
2. **Outcome-from-`start` proof (AC-12)**: run the full `register → stop → import --slug → start` sequence; after `start`, issue a vector search against the restored slug and assert it returns the restored corpus's semantic hits — proving the daemon loaded the *rebuilt* index, not a stale one. Prove from `start` onward, **not** from disk state.
**Coverage Requirement**: The restored-vector outcome is asserted through a served query after daemon restart. A test that only stats `{slug}/vector` for a fresh HNSW file (AC-02) is necessary but does NOT discharge SR-10.

### R-04: Round-trip losslessness into a fresh slug (High) — AC-10
**Test Scenarios**:
1. Seed slug store A via literal-slug layout with rows exercising every table (entries + 26 columns, entry_tags, co_access, feature_entries, graph_edges, audit_log, counters) → `export --slug A` → `import --slug B` into a **second, freshly-registered** slug B → diff all tables A vs B.
2. Type-fidelity assertions: f64 confidence **bit-exact**, JSON-in-TEXT emitted raw (not re-encoded), NULL vs empty-string preserved, content-hash + chain-link `chain_verify` clean report.
**Coverage Requirement**: All round-tripped tables compared; B is audit-empty at import so the explicit-`event_id` INSERT cannot collide (C-5 basis). Round-trip crosses **two different slugs**, not A→A.

### R-05 / R-06: Deploy-shape base derivation + host bind-mount (High) — C-1, NFR-3, SR-11
**The four shapes are a coverage axis, not one representative.**
**Test Scenarios**:
1. `*_with_base` hook: `data_dir.parent() == X` verbatim; `resolve_slug_store` joins `X/<slug>` (the AC-09 vehicle) — correct-resolve asserted.
2. In-container idiom (`HOME=/data`, base `None`→`/data/.unimatrix`): assert `data_dir.parent()` == `/data/.unimatrix` and the slug resolves under it. (May be asserted at the derivation-unit level to avoid a container in CI.)
3. Local dev (`None`→`~/.unimatrix`): derivation-unit assertion that parent == base.
4. **Host bind-mount fail-loud corner**: base resolves host `$HOME/.unimatrix` where the slug store does **not** exist → `--slug` fails loud (surfaces as AC-03 missing-store) naming the **host** resolved absolute path — which is what distinguishes a base miss from a typo. Assert it never no-ops and never resolves the container store.
**Coverage Requirement**: Each of the four shapes either resolves correctly or fails loud with the resolved path. The `None` base fallback is exercised for a `data_dir.parent() == None` input without `unwrap` (NFR-4).

### R-07: Non-empty-audit restore refusal (High) — AC-10/FR-13, C-5, ADR-005
**Test Scenarios**:
1. `import --slug` into a slug store whose `audit_log` already has rows → assert pre-flight refusal **before** `drop_all_data`/insert, message directs "register a fresh slug", and the raw SQLite UNIQUE error is **never** surfaced.
2. Confirm `--force` does not bypass it (no `--force` override exists for this refusal).
**Coverage Requirement**: The refusal fires on `audit_log` non-empty (append-only, `drop_all_data` can't clear it), pre-flight, with the actionable message.

### R-08: Validation bypass / traversal (High) — AC-04, C-2, NFR-8
**Test Scenarios**:
1. Parameterized reject set at the CLI edge **before any FS/DB access**: charset-invalid (`Foo!`, `a_b`, 64+ chars, leading `-`, uppercase), reserved (`v1`, `health`, `observe`, `tools`).
2. Traversal attempts (`../x`, `%2e%2e`, `/abs`, `a/b`, embedded NUL) rejected at `ProjectSlug::try_from` — assert unrepresentable, no filesystem touch.
**Coverage Requirement**: Only a `&ProjectSlug` crosses into `per_slug_data_dir` (one join site); rejection leaves zero filesystem side effects. Traversal is closed **structurally**, not by runtime sanitization.

### R-09: No-`--slug` fallthrough parity (High) — AC-05, NFR-1
**Test Scenarios**:
1. Existing export/import integration suites pass unchanged.
2. Fallthrough assertion: no-slug resolved path == the path-hash `data_dir` (funnel not entered when `slug=None`).
**Coverage Requirement**: Verified as a **property** (resolved path identity), not by one example.

### R-10: Silent sparse export (Med) — AC-06, SR-08
**Test Scenarios**:
1. Capture stderr on export; assert it contains entry count, audit-row count, and resolved output path; stdout unaffected.
2. Export a store with 0 knowledge entries but audit rows → stderr reads `exported 0 entries, M audit rows` (self-diagnosing). The `--skip-quarantined`/audit asymmetry filter is NOT changed.

### R-11: Live-PID gate correctness (Med) — AC-13, ADR-003
**Test Scenarios**:
1. Live PID present at path-hash `pid_path` → `import --slug` refuses, message names resolved PID path + `stop → import → start` remedy.
2. Predicate is **live-PID-only**: a `[[projects]]` stanza written by `register` but no live daemon does NOT block import (else the canonical sequence is refused). Assert import proceeds when PID absent/dead even with the stanza present.
**Coverage Requirement**: Liveness uses `is_process_alive`/`is_unimatrix_process`; a stale/dead PID does not block.

### R-12: Restore-sequence discoverability (Med) — AC-12/FR-16, SR-07
**Test Scenarios**: README assertion that the canonical `register → stop → import --slug → start` sequence is present; both commands' `--slug` help present, import's help carries the README pointer (AC-07).

### R-13: `--slug` resolves stray/hash dir (Low→Med) — AC-11, SR-04, ADR-002
**Test Scenarios**: Seed both a populated slug dir and the hash store under one base; assert no-`--slug` export emits only the hash store's rows (boundary guard, documented not silently reinterpreted).

### R-14: Partial-failure side effects (Med) — AC-03
**Test Scenarios**: On each fail-loud path (missing store, invalid slug, non-empty audit, live PID), assert no partial db, no dirs, no output file left behind.

## Behavioral-Outcome Coverage (from the scope behavioral lens)

Every entry point + outcome from SCOPE-RISK-ASSESSMENT drives the operator's real CLI invocation, not a seam beneath it.

| Outcome (scope lens) | Entry point | Required scenario (drives the real entry point, asserts the outcome) |
|---|---|---|
| `<file>` holds the slug's actual corpus (not sibling/hash); stderr summary reflects it | `export --slug` | R-01 S1 (disagreement seam) + R-10 S1 — emitted rows == slug A, ∩ hash B == ∅, stderr counts match A |
| After the restart sequence, that slug serves the restored corpus incl. vector search | `import --slug` (full sequence) | R-03 S2 — `register→stop→import→start`, then a served vector query returns restored hits |
| No-`--slug` byte-for-byte identical | `export`/`import` (no flag) | R-09 S1+S2 — suites unchanged + resolved path == path-hash `data_dir` |
| Missing store → fails loud, creates nothing, names resolved absolute path + next action | `export/import --slug` (no store) | R-02 S1 + R-14 — non-zero exit, absolute path in error, FS unchanged |
| Charset-invalid/reserved → rejected at CLI edge before any FS/DB | `export/import --slug <bad>` | R-08 S1 — parameterized reject, zero FS side effects |
| Restored slug serves vector search after `start`; rebuilt index is the one loaded | `register→stop→import→start` | R-03 S2 — proven from `start`, not disk state |
| Live daemon PID → hard-errors naming PID path + remedy, never partial/clobbered restore | `import --slug` (daemon up) | R-11 S1 + R-03 S1 — refusal message asserted; no write to `{slug}/vector` |

A test that proves an outcome one layer beneath the operator's entry point (e.g. asserting `resolve_slug_store` returns the right `PathBuf`, or that a fresh HNSW file exists on disk) is **necessary but not sufficient** — the required scenario must drive `run_export_with_base`/`run_import_with_base` (or the full CLI sequence) and assert the observed outcome.

## Integration Risks

- **Funnel ↔ `ensure_data_directory` coupling (C-6)**: the path-hash `data_dir`+`vector/` are still created/chmod'd before `db_path` is discarded in slug mode. Test that slug-mode export/import does not fail because of, nor depend on the *contents* of, that discarded hash dir — but also that the hash dir's incidental creation is not mistaken for the slug store.
- **Vector rebuild target redirect (ADR-004)**: `reconstruct_embeddings` must receive `slug_dir/"vector"`, not `paths.vector_dir`. Integration test asserts the fresh HNSW lands under `{base}/<slug>/vector` and nothing is written to the hash `vector/`.
- **PID path stays base-scoped in slug mode (FR-11)**: import reads the daemon PID from the path-hash `paths.pid_path`, not a per-slug path. Assert the gate consults the correct (one-daemon) PID.
- **Sync pre-tokio + multi-thread runtime (C-8)**: import keeps its multi-thread runtime (`block_in_place`/GH#554); a `current_thread` regression panics only at the reconstruct step — cover with the AC-02/AC-10 import running to completion.

## Edge Cases

- `data_dir.parent()` returns `None` (base at filesystem root) → fallback idiom, no `unwrap`, fail loud.
- Slug dir exists but `unimatrix.db` absent (only `vector/` present) → treated as missing store (existence gate is on the db file).
- Export against a live daemon's slug store (AC-08): read-only alongside WAL + `busy_timeout` succeeds, no locking added (#2621 open_readonly analogue).
- Reserved slug that is also a real hash-dir-name collision — reserved check still rejects at the edge.
- Import into a freshly-registered but empty slug store (0 rows, audit-empty) — the supported target; round-trip succeeds.
- Slug at max length (63 bytes) and min (1 byte) — boundary of `ProjectSlug::try_from`.

## Security Risks

Untrusted input surface: the `--slug` raw `&str` from the operator's CLI.
- **Path traversal / absolute-path escape**: closed **structurally** at `ProjectSlug::try_from` — `.`, `/`, `\`, `%`, whitespace, NUL, uppercase are unrepresentable; ASCII-only makes the 1..=63 byte bound exact (no multi-byte bypass). Blast radius if bypassed: read/write of an arbitrary DB path under (or outside) the base. Test R-08 S2 asserts the closure; any code path admitting a raw `&str` into `per_slug_data_dir` is a finding.
- **Reserved-name confusion**: reserved slugs (`v1`, `health`, `observe`, `tools`) rejected so operator cannot address runtime-reserved dirs.
- **Auto-create-as-write (C-3)**: `SqlxStore::open` is a write; reaching it on an attacker-influenced nonexistent path would materialize a store. Existence gate before `open` bounds this.
- **Import is a destructive write** (`drop_all_data`): gated by live-PID refusal (no concurrent daemon) and non-empty-audit refusal (no clobber of accumulated history). Blast radius bounded to a fresh, offline slug store.
- No new external input surface, no deserialization beyond the existing JSONL import path, no network.

## Failure Modes

Every accept-but-inert path fails loud with the **fully-resolved absolute path** and the next action — no silent no-op, no auto-create, no raw SQLite error to the operator:

| Failure | Behavior | AC |
|---|---|---|
| Missing store at resolved path | non-zero exit, names absolute path + next action, creates nothing | AC-03 |
| Charset-invalid/reserved slug | reject at CLI edge, no FS/DB touch | AC-04 |
| Host base miss (bind-mount) | fail loud naming the *host* resolved path (distinguishes base miss from typo) | SR-11/C-7 |
| Live daemon PID present (import) | hard-error, names PID path + `stop→import→start` | AC-13 |
| Non-empty `audit_log` target (import) | pre-flight refusal, "register a fresh slug", never raw UNIQUE | AC-10/FR-13 |
| Sparse/empty export | success + stderr `exported 0 entries, M audit rows` (self-diagnosing) | AC-06 |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution / Coverage |
|-----------|------------------|-----------------------|
| SR-01 (two resolvers, wrong resolve returns a real store) | R-01 | AC-09 disagreement seam; single funnel `resolve_slug_store` (ADR-001); N=1 same-path test explicitly rejected as ceremonial (#4974) |
| SR-02 (`open` auto-creates before existence check) | R-02 | Existence gate strictly before `open` (C-3/ADR-002); FS-unchanged assertion |
| SR-03 (live-daemon vector clobber) | R-03, R-11 | Live-PID-only hard-error makes clobber structurally unreachable (ADR-003); outcome proven from `start` |
| SR-04 (file-existence vs registration gate) | R-13 | Gate on file existence; AC-11 boundary guard; help text documents "store dir, not registered project" |
| SR-05 (non-empty audit restore target) | R-07 | Pre-flight refusal with actionable message (ADR-005/C-5); `--force` does not bypass |
| SR-06 (scope-creep: other 6 CLIs) | — | Out of scope; pattern established here for them to copy. No test; process/review boundary |
| SR-07 (restore sequence load-bearing, multi-step) | R-12 | README canonical (AC-12/FR-16); import help pointer (AC-07) |
| SR-08 (round-trip hash validity; audit-only export legitimate) | R-04, R-10 | Chain/hash validation on emitted rows (AC-10); stderr count summary self-diagnoses (AC-06); filter untouched |
| SR-09 (export --slug seam divergence) | R-01 | AC-09 — drives `run_export_with_base(slug=…)`, asserts slug rows + none of hash rows |
| SR-10 (import rows land but stale vector served) | R-03 | Proven from `start` onward — served vector query post-restart (AC-12), not disk state |
| SR-11 (host bind-mount silent miss) | R-06 | Fail loud with resolved *host* path (C-7/ADR-006); four-shape coverage axis; never no-op |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | 6 scenarios — AC-09 disagreement seam (top weight), pre-open existence gate + FS-unchanged, live-PID unreachability + served-vector-from-`start` |
| High | 6 (R-04..R-09) | ~11 scenarios — full round-trip across all tables, four-shape axis incl. host-bind-mount fail-loud, non-empty-audit refusal, traversal/validation reject set, no-slug parity property |
| Medium | 5 (R-10..R-14) | ~7 scenarios — sparse-export stderr, PID gate correctness (live-only predicate), README/help discoverability, stray-hash-dir boundary, partial-failure no-side-effects |
| Low | included above | — |

**Non-negotiable for gate**: R-01 S1 (AC-09 disagreement seam with disjoint non-empty hash-store set B, seeded via runtime layout / read via CLI resolver) and R-03 S2 (served vector search after the full `register→stop→import→start` sequence). Absent either, the feature is unproven for the personal-cloud shape the goal names as the destination — regardless of how many same-path/disk-state tests pass.

## Knowledge Stewardship
- Queried: context_search "two resolvers path-hash slug disagree seam" — surfaced #4974 (ceremonial-seam/N=1 false confidence), #4950 (single-funnel resolver ADR vnc-034), #5087, #5344 (ADR-004 single-restart two-slug), #4962; and "backup restore round-trip vector clobber" (pattern) — #2676 (VectorIndex snapshot round-trip test pattern), #2621 (open_readonly no-migrate, AC-08 analogue), #2673/#3764 (vector-dir sibling load / restore re-insert). All applied to R-01/R-03/R-04/edge cases.
- Declined: nothing novel to store — the operative pattern (seam ahead of 2nd impl is ceremonial unless it carries value; #4974) and the two-resolver trap (#5507) already exist; this feature's risks are specific interpretations, not a new cross-feature pattern. Promotable at retro only if the four-deploy-shape-as-coverage-axis recurs in the sibling CLI slug-awareness work.
