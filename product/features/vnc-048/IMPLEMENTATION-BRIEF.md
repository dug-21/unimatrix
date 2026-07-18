# vnc-048 Implementation Brief — Per-Slug Backup/Restore for Personal-Cloud

Compiled from the approved Session 1 design. Session 2 agents consume this brief; open the source documents for full rationale. GH Issue #953.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-048/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-048/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-048/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-048/architecture/ARCHITECTURE.md |
| Risk / Test Strategy | product/features/vnc-048/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-048/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-048/ACCEPTANCE-MAP.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| resolve_slug_store (funnel, projects.rs) | pseudocode/resolve_slug_store.md | test-plan/resolve_slug_store.md |
| export.rs (slug branch + stderr summary) | pseudocode/export.md | test-plan/export.md |
| import/mod.rs (slug branch + pre-flight gates + vector redirect) | pseudocode/import.md | test-plan/import.md |
| main.rs (clap `--slug` wiring) | pseudocode/main-dispatch.md | test-plan/main-dispatch.md |
| README (canonical restore procedure) | pseudocode/readme.md | test-plan/readme.md |

Pseudocode and test-plan files produced in Stage 3a — all paths above verified present on disk (Delivery Leader, Component Map update). No shared runtime/HTTP path is modified (C-9).

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Add `--slug <name>` to the operator `export` and `import` CLI subcommands so a personal-cloud operator can back up and restore a named per-slug project's actual knowledge corpus — resolving the target store through the **runtime's** literal-slug scheme (`{base}/<slug>/unimatrix.db`), not the CLI's path-hash scheme. This closes the silent two-resolvers gap where both commands target (and auto-create) a near-empty path-hash store and report success. Every failure in the new paths is loud, names the fully-resolved absolute path, and states the next action; without `--slug`, behavior is unchanged.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Slug-store address derivation | Reuse triad — one base derivation (`data_dir.parent()`), one join site (`per_slug_data_dir`, `&ProjectSlug` only), one validation edge (`validate_slug`); no second scheme, no `--base` flag/env | SR-01, C-1/C-2/C-6 | architecture/ADR-001-reuse-triad-single-scheme.md |
| Resolvability gate | `db_path.exists()` strictly before `SqlxStore::open`; gate is file-existence, not `[[projects]]` registration (de-registered project is exportable) | SR-02/SR-04, C-3 | architecture/ADR-002-preopen-existence-gate.md |
| Live-daemon import safety | `import --slug` hard-errors on a live daemon PID (live-PID-only predicate; `[[projects]]` half dropped); makes the shutdown vector-clobber structurally unreachable. No `--force` override | SR-03, C-4, OQ-1 | architecture/ADR-003-live-pid-only-import-refusal.md |
| Vector rebuild target | Import rebuilds HNSW into `slug_dir/vector`; `pid_path` stays base-scoped path-hash (daemon's PID) | SR-10, OQ-1 | architecture/ADR-004-vector-rebuild-into-slug-dir.md |
| Restore target constraint | Pre-flight refuse when destination `audit_log` is non-empty; supported target is a freshly-registered (audit-empty) slug; never surface raw SQLite UNIQUE | SR-05, C-5, OQ-2 | architecture/ADR-005-fresh-slug-audit-empty-target.md |
| Closing the silence | Fail-loud naming the fully-resolved absolute path on every accept-but-inert path; export stderr count summary (`exported N entries, M audit rows → <path>`); no heuristic sibling-dir scan | SR-08/SR-11, C-7, OQ-4 | architecture/ADR-006-fail-loud-resolved-path-and-export-summary.md |
| Restore procedure home | README is canonical for `register → stop → import --slug → start`; import `--slug` help points to it | SR-07, OQ-3 | (SCOPE OQ-3 / SPEC FR-16) |

## Files to Create / Modify

| File | Change |
|------|--------|
| `unimatrix-engine/src/projects.rs` | New `resolve_slug_store` funnel + `SlugStorePaths` struct; raise `per_slug_data_dir` and `ProjectRegistry::validate_slug` to `pub(crate)` |
| `.../export.rs` | Add `slug: Option<&str>` to `run_export`/`run_export_with_base`; branch to funnel; stderr count summary (AC-06, both modes) |
| `.../import/mod.rs` | Add `slug: Option<&str>` to `run_import`/`run_import_with_base`; branch to funnel; live-PID hard-error gate; non-empty-audit pre-flight refusal; redirect vector rebuild to `slug_dir/vector` |
| `main.rs` | Clap `--slug` arg on `Export`/`Import`; thread into `run_*` calls (`main.rs:556-567`) |
| `README` | New canonical restore-procedure section |
| `tests/export_integration.rs`, import integration tests | AC-09 seam test, AC-10 round-trip, and the rest of the coverage matrix |

## Data Structures

```rust
// projects.rs — single slug-store resolution funnel (pub(crate))
pub(crate) struct SlugStorePaths {
    pub slug_dir: PathBuf,
    pub db_path: PathBuf,     // slug_dir/"unimatrix.db"
    pub vector_dir: PathBuf,  // slug_dir/"vector"
}
```

`ProjectPaths` (existing, `project.rs:176-186`): `data_dir, db_path, vector_dir, pid_path: PathBuf` — all path-hash-scoped. In slug mode, DB/vector come from `SlugStorePaths`; `pid_path` stays from `ProjectPaths` (ADR-004).

## Function Signatures (integration surface — invent none)

```rust
// NEW
pub(crate) fn resolve_slug_store(
    paths: &ProjectPaths,
    raw_slug: &str,
) -> Result<SlugStorePaths, ServerError>;   // ServerError: std::error::Error → Box<dyn Error>

// RAISED to pub(crate) — bodies unchanged
fn per_slug_data_dir(base: &Path, slug: &ProjectSlug) -> PathBuf;         // projects.rs:123 — ONLY join site
fn ProjectRegistry::validate_slug(raw: &str) -> Result<ProjectSlug, ServerError>; // projects.rs:206 — ONLY validation edge

// REUSED unchanged
fn ensure_data_directory(override_dir: Option<&Path>, base_dir: Option<&Path>) -> io::Result<ProjectPaths>; // project.rs:146
impl TryFrom<&str> for ProjectSlug;  // http/router/seam.rs:96-118 — charset ^[a-z0-9][a-z0-9-]{0,62}$, ASCII, 1..=63
fn is_reserved_slug(slug: &ProjectSlug) -> bool;   // infra/config.rs:2498 — v1, health, observe, tools
fn read_pid_file(path: &Path) -> Option<u32>;      // infra/pidfile.rs:93
fn is_process_alive(pid: u32) -> bool;             // infra/pidfile.rs:113  (kill -0)
fn is_unimatrix_process(pid: u32) -> bool;         // infra/pidfile.rs:141  (/proc/{pid}/cmdline)

// SIGNATURE CHANGE — add slug param
run_export(..., slug: Option<&str>);   run_export_with_base(..., slug: Option<&str>);   // export.rs:32,46
run_import(..., slug: Option<&str>);   run_import_with_base(..., slug: Option<&str>);   // import/mod.rs:54,68

// REDIRECTED target in slug mode
reconstruct_embeddings(slug_vector_dir /* was &paths.vector_dir */, ...);  // import/mod.rs:226, embed_reconstruct.rs:110
```

### Base derivation idiom (C-1, NO unwrap)

```rust
let base = paths.data_dir.parent()
    .map(Path::to_path_buf)
    .unwrap_or_else(|| paths.data_dir.clone());   // existing at projects.rs:181-185, main.rs:1287
```

### Slug-mode funnel order (the core contract)

```
validate_slug(raw)            # ProjectSlug::try_from + is_reserved_slug — CLI edge, before any FS/DB (AC-04)
→ base = data_dir.parent()    # no unwrap (C-1)
→ slug_dir = per_slug_data_dir(base, &slug)   # only join site (C-2)
→ db_path = slug_dir/"unimatrix.db"; vector_dir = slug_dir/"vector"
→ EXISTENCE GATE: db_path.exists()? else fail loud naming absolute path (C-3, AC-03)
→ (import only) live-PID gate (ADR-003) → non-empty-audit gate (ADR-005)
→ SqlxStore::open(db_path)     # reached ONLY after existence gate — open is never the gate (SR-02)
```

Without `--slug` the funnel is not entered; `paths.db_path` (path-hash) flows exactly as today.

## Constraints

- **C-1** Base MUST be `paths.data_dir.parent()` via the existing fallback idiom, no `unwrap`. No new base surface.
- **C-2** `per_slug_data_dir` is the only join site; `&ProjectSlug` (never `&str`) crosses in. Traversal closed **structurally** at `ProjectSlug::try_from` (`.`, `/`, `\`, `%`, whitespace, NUL, uppercase unrepresentable).
- **C-3** Existence check strictly before `SqlxStore::open` on both paths.
- **C-4** (import) Live-PID-only refusal makes the shutdown vector-clobber structurally unreachable.
- **C-5** (import) `drop_all_data` cannot clear append-only `audit_log`; supported target is a freshly-registered (audit-empty) slug; else fail loud with actionable message.
- **C-6** `ensure_data_directory` still creates/chmods the path-hash `data_dir` + `vector/` before its `db_path` is discarded in slug mode — accepted, do not optimize away.
- **C-7** Host-side `--slug` resolves host `$HOME` base and misses → fail loud with the resolved path (one line of help text).
- **C-8** Both commands stay sync pre-tokio subcommands; import keeps its multi-thread runtime (`block_in_place` in `embed_reconstruct` panics on `current_thread`, GH#554).
- **C-9** Signature changes touch `main.rs:556-567` and the two integration test files only; no shared runtime path modified.
- **C-10** No `.unwrap()`/`.expect()` in non-test code; `cargo fmt` / `clippy -D warnings` clean; max 500 lines/file.

## Dependencies

- `unimatrix-engine` — `per_slug_data_dir` (→ `pub(crate)`), `ProjectRegistry::validate_slug` (→ `pub(crate)`), `ProjectSlug::try_from`, `is_reserved_slug`, `ensure_data_directory`, `ProjectPaths`.
- `unimatrix-store` — `SqlxStore::open`, `import/inserters::insert_audit_log`, `import/mod` (`reconstruct_embeddings`, `print_summary`, `drop_all_data`), export corpus reader.
- `infra/pidfile.rs` — `read_pid_file`, `is_process_alive`, `is_unimatrix_process` (reused, unchanged).
- Binary crate `main.rs` — `run_export`/`run_import` (+ `_with_base`) signatures, clap arg wiring.
- README — canonical restore-procedure section.
- No new crates, no external services.

## NOT in Scope

- The `--skip-quarantined` / `audit_log` filter asymmetry — correct as designed, not touched (an audit-rows-only export stays a legitimate output).
- A new base-resolution mechanism (`--base` flag, env var, second scheme) — explicitly refused.
- Slug-awareness for the other six CLIs (`verify`, `snapshot`, `eval`, `health`, `stop`, `client-bundle`) — this feature only establishes the `--slug` + `resolve_slug_store` pattern they may copy.
- Restoring over a slug store with existing audit history — fails loud, out of scope.
- Live-daemon import (locking, daemon-mediated import, index invalidation) — separate design problem.
- Backup as disaster recovery — the volume-snapshot DR story is unchanged.
- Version skew (exporter/importer newer than the daemon) — pre-existing hazard; exec-into-container mitigation applies.
- A `--force`-style override for the live-PID or non-empty-audit refusals.
- An import count summary (import already prints per-table counts).
- `#5586` capability retag (OQ-5) — owned by the vision session, not this feature.

## Alignment Status

Vision guardian: **PASS 5 / WARN 1 / FAIL 0** (VARIANCE 0). All 5 SCOPE goals and 13 ACs carried into spec FRs + verification and architecture ADRs; nothing dropped. Milestone discipline honored (no forward over-build). Single-funnel invariant, append-only-audit, and hash-chain principles reinforced.

**WARN-1 — export stderr summary on the no-`--slug` path (RECONCILED, do not re-open).** Spec FR-8 emits the stderr count summary on both slug and no-slug export; SCOPE AC-05/NFR-1 promise the no-`--slug` path is "byte-for-byte identical." Adding stderr is an observable change to that path. **Reconciliation adopted for delivery:** AC-05's byte-for-byte guarantee is scoped to the **exported file + stdout + exit code** — stderr is explicitly excluded. This is faithful to SCOPE (which declares the summary "not a behavior change"). Delivery obligations:
1. AC-05 verification asserts identity of the exported file, stdout, and exit code — not stderr.
2. Confirm no existing export/import integration test asserts on empty/absent stderr; if one does, update it to permit the summary line (it is not a regression). R-09 test-plan must carry this one-line check.

**ADVISORY (vision-session action, not a delivery task):** capability `#5586` (BACKUP-RESTORE) is `delivery:proven` but proven only for local single-project. Once vnc-048 delivers AC-09 + AC-10 evidence, the vision session should flip `#5586 → partial`, tighten `proven_by` to name the resolver and shape, and restore `proven` only for the covered shape (via `context_correct`). Not restored on an export-only fix. Recorded as OQ-5.

## Coordination Notes (wave / ordering)

- **Foundation first (Wave A):** `resolve_slug_store` + `SlugStorePaths` + the two `pub(crate)` visibility raises land before export/import branches — both commands depend on the funnel. `export.rs` and `import/mod.rs` slug branches can then proceed in parallel; `main.rs` clap wiring depends on both signatures.
- **Gate non-negotiables (from Risk Strategy):** the feature is unproven for the personal-cloud destination without **AC-09 disagreement seam** (seed via runtime literal-slug layout, read via CLI resolver, hash-store set B non-empty and disjoint — an N=1 same-path test is ceremonial, #4974) and **AC-12 served-vector-from-`start`** (full `register → stop → import → start`, then a served vector query). No count of same-path or disk-state tests substitutes.
- **`_with_base` wrinkle is load-bearing:** `ensure_data_directory(_, Some(X))` sets `unimatrix_base = X` verbatim, so `data_dir.parent() == X` and the funnel joins `X/<slug>` — the same path `http_provision` writes. The AC-09 seam test relies on this.
- **Two path sources in import slug mode** (DB/vector from `SlugStorePaths`, PID from `paths`) must not be "tidied" into one — redirecting PID to `slug_dir` breaks the ADR-003 live-PID gate.
