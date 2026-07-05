# nxs-014 Architecture — Cross-Version Hash Chain (Weak Mode) + Chain-Verify Core

> Scope is SETTLED (D-1..D-4). This document translates the settled scope into component
> boundaries, a crate-placement decision for the shared verify core, and the exact
> integration surface downstream agents must implement against. It invents no new policy;
> where scope froze a decision (weak mode, frozen hash, forward-only legacy) this document
> reaffirms and locates it in code.

## System Overview

`context_correct` writes a successor entry that supersedes the original. The cross-version
`previous_hash` column exists in the schema but the correction write path hardcodes
`previous_hash = ""` / `version = 1`, so the "tamper-recorded correction chain" the product
promises is never actually populated (GH #912).

nxs-014 does three things, all in **weak mode** (no hash-format change, no anchor, no migration):

1. **Populate the link on correction** — `previous_hash = superseded.content_hash`,
   `version = superseded.version + 1`, in `unimatrix-store`.
2. **Factor a transport-agnostic chain-verify core** that walks the supersedes chain and
   fails loud when a link or content hash is inconsistent — placed in `unimatrix-store` so
   the CLI (today) and a future enterprise admin MCP tool (post-RBAC) are both thin callers.
3. **Expose the core via a new `verify` CLI subcommand** in `unimatrix-server`, and refactor
   the import-time `validate_hashes` to reuse the same core (one integrity oracle, not two).

The threat model is **tamper-RECORDED, not tamper-evident** against a database-write adversary
(that adversary is out of tier — it owns the bearer token and all secrets). The chain catches
accidental corruption and API-surface tamper. This boundary is pinned in ADR-002 and the README
so no downstream agent silently re-upgrades the claim.

## Component Breakdown

| Component | Crate | Responsibility | New / Changed |
|-----------|-------|----------------|---------------|
| Correction write path | `unimatrix-store` `write_ext.rs` | Populate `previous_hash`/`version` from the loaded `original` at both the struct literal and the INSERT binds | Changed (2 sites) |
| **Chain-verify core** | `unimatrix-store` `chain_verify.rs` (new module) | Pure, connection-agnostic integrity oracle over a slice of `EntryRecord`: content-hash recompute + supersedes-link check, skipping empty `previous_hash` legacy | New |
| Import hash validation | `unimatrix-server` `import/mod.rs` `validate_hashes` | Thin adapter: load entries from the in-flight import transaction, call the store core, map violations to the import error | Refactored |
| `verify` CLI subcommand | `unimatrix-server` `main.rs` + `verify.rs` (new module) | Resolve project dir → db path, open a read-only store, call the store core, print the report, set exit code | New |
| README integrity section | repo root `README.md` | State the true, threat-model-scoped guarantee (ADR-002) | Changed |

### Why the verify core lives in `unimatrix-store` (the load-bearing decision — SR-03, D-4)

`unimatrix-store` is a leaf crate: it depends on no other `unimatrix-*` crate; `unimatrix-server`
depends on it. Every input the verify core needs already lives in store —
`compute_content_hash`, `EntryRecord`, `entry_from_row`, `query_all_entries`,
`query_supersession_chain`. Placing the core in store means:

- **No dependency cycle.** Both callers that exist today (import) and tomorrow (a server-side
  MCP admin tool, both in `unimatrix-server`) depend on store already. A future non-server caller
  could also reach it. Placing it in server would strand any non-server consumer and leave the
  core wedged next to the import pipeline it happens to be called from first.
- **One oracle enforces the AND.** AC-04 requires the content-hash recompute AND the chain-link
  check to run together. A single pure `verify_entries(&[EntryRecord])` in store makes it
  impossible for a caller to run only half the check. `validate_hashes` becomes a thin adapter,
  not a second implementation that can drift (the vnc-034 single-oracle lesson).

Full rationale and the rejected server-placement alternative are in **ADR-001**.

## Component Interactions / Data Flow

```
correction (context_correct)
  store::write_ext::correct_entry
    original = read(original_id)            // already loaded at write_ext.rs:462
    new_rec.previous_hash = original.content_hash     // struct :539
    new_rec.version       = original.version + 1      // struct :540
    INSERT ... .bind(&new_rec.previous_hash)          // bind :582
               .bind(new_rec.version as i64)          // bind :583

verify (two callers, one core)
  CLI:    server::verify::run_verify(project_dir)
            paths = project::ensure_data_directory(project_dir, None)
            store = SqlxStore::open_readonly(paths.db_path)
            entries = store.query_all_entries()        // ALL statuses incl. Deprecated
            report  = store::chain_verify::verify_entries(&entries)   // PURE CORE
            print(report); exit(if report.is_clean() {0} else {non-zero})

  import: server::import::validate_hashes(&mut conn)   // inside BEGIN IMMEDIATE, pre-COMMIT
            entries = load all rows on `conn` via ENTRY_COLUMNS + entry_from_row
            report  = store::chain_verify::verify_entries(&entries)   // SAME PURE CORE
            if !report.is_clean() { ROLLBACK; Err(report) }
```

The pure core (`verify_entries`) is the transport-agnostic heart D-4 mandates. It touches no
I/O: each caller supplies the entry set from its own connection (import must use its in-flight
transaction connection so it sees uncommitted rows; the CLI uses a read-only pool). This is what
lets a future MCP tool wrap it with zero new logic — it hands the core the entries it already has.

### Verify algorithm (corpus-wide, O(n))

Build `HashMap<u64, &EntryRecord>` keyed by `id`. For each entry:

1. **Content hash:** recompute `compute_content_hash(title, content)`; if `!= content_hash`,
   emit `ContentHashMismatch { computed, stored }`. (Catches naive content mutation — AC-04.)
2. **Chain link:** if `previous_hash` is empty → skip as unverifiable-legacy (SR-02, D-2, AC-03),
   increment `skipped_legacy`. Otherwise resolve the predecessor by `supersedes`:
   - `supersedes == None` → `DanglingPreviousHash` (a link with no chain edge; should not occur
     by construction, fail loud rather than ignore).
   - predecessor id not in map → `MissingPredecessor { predecessor_id }`.
   - `predecessor.content_hash != previous_hash` → `ChainLinkMismatch { predecessor_id,
     expected: predecessor.content_hash, found: previous_hash }`. (AC-03.)

`ChainReport` names every offending entry id (AC-04, SR-05). The verify walk keys the link check
on the authoritative `supersedes` edge (Non-Goal 6 — the chain itself is trusted, A-2), which is
strictly stronger than the pre-existing "previous_hash references *some* known hash" existence
check and subsumes it.

## Technology Decisions (ADRs)

| ADR | Title | Settles |
|-----|-------|---------|
| ADR-001 | Chain-verify core in `unimatrix-store`; server `validate_hashes` + CLI as thin callers | SR-03, D-4 |
| ADR-002 | Weak-mode threat boundary: tamper-recorded, `compute_content_hash` frozen, no cascade/anchor/migration | D-1, SR-01, SR-04, AC-06, AC-07 |
| ADR-003 | Correction chain population (both literal sites) + chain-verify semantics (skip empty legacy, fail loud) | D-1 write path, SR-02, SR-06, AC-01..04 |

## Integration Surface

New and existing interfaces downstream agents implement against. Do not invent names or types.

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `compute_content_hash` | `pub fn compute_content_hash(title: &str, content: &str) -> String` — **FROZEN** (ADR-002) | `unimatrix-store/src/hash.rs:7` |
| `EntryRecord` chaining fields | `content_hash: String`, `previous_hash: String`, `version: u32`, `supersedes: Option<u64>`, `superseded_by: Option<u64>`, `id: u64`, `status: Status` | `unimatrix-store/src/schema.rs:49` |
| `entry_from_row` | `pub fn entry_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<EntryRecord>` | `unimatrix-store/src/read.rs:20` |
| `ENTRY_COLUMNS` | `pub const ENTRY_COLUMNS: &str` (all entry columns, DDL order) | `unimatrix-store/src/read.rs:11` |
| `query_all_entries` | `pub async fn query_all_entries(&self) -> Result<Vec<EntryRecord>>` — **must return ALL statuses incl. Deprecated** (predecessors are Deprecated) | `unimatrix-store/src/read.rs:324` |
| `SqlxStore::open_readonly` | `pub async fn open_readonly(path: impl AsRef<Path>) -> Result<SqlxStore>` | `unimatrix-store/src/db.rs:147` |
| `project::ensure_data_directory` | `fn ensure_data_directory(project_dir: Option<&Path>, base_dir: Option<&Path>) -> Result<ProjectPaths>` (yields `db_path`) | `unimatrix-server/src/project` |
| Correction path `original` | loaded `EntryRecord` with populated `content_hash`/`version` | `unimatrix-store/src/write_ext.rs:462` |
| **NEW** `verify_entries` (pure core) | `pub fn verify_entries(entries: &[EntryRecord]) -> ChainReport` | `unimatrix-store/src/chain_verify.rs` (new) |
| **NEW** `ChainReport` | `pub struct ChainReport { pub checked: usize, pub skipped_legacy: usize, pub violations: Vec<ChainViolation> }` + `pub fn is_clean(&self) -> bool` + `Display`/`describe()` | `unimatrix-store/src/chain_verify.rs` (new) |
| **NEW** `ChainViolation` | `pub struct ChainViolation { pub entry_id: u64, pub kind: ViolationKind }` | `unimatrix-store/src/chain_verify.rs` (new) |
| **NEW** `ViolationKind` | `enum { ContentHashMismatch{computed,stored}, ChainLinkMismatch{predecessor_id,expected,found}, MissingPredecessor{predecessor_id}, DanglingPreviousHash }` | `unimatrix-store/src/chain_verify.rs` (new) |
| **NEW** verify handler | `pub fn run_verify(project_dir: Option<&Path>) -> Result<(), Box<dyn std::error::Error>>` (sync wrapper spinning a runtime, mirroring `run_import`) | `unimatrix-server/src/verify.rs` (new) |
| **NEW** CLI variant | `Command::Verify { }` dispatched in the pre-Tokio sync block of `fn main()` | `unimatrix-server/src/main.rs` |

### Error boundaries

- Store core returns a `ChainReport` value (not an error) — violations are data, not failures;
  the report is always produced. Loaders return `unimatrix_store::Result` (`StoreError`).
- Import maps a non-clean report to its existing `Box<dyn std::error::Error>` and ROLLBACKs
  before COMMIT (existing control flow at `import/mod.rs:212`), preserving the "reject bad
  import atomically" guarantee.
- CLI `run_verify` returns `Err` on a non-clean report; `fn main` prints and exits non-zero
  (matches the existing subcommand → exit-code pattern). Exit 0 = clean, non-zero = break found.
- No `.unwrap()` in non-test code; all fallible I/O uses `?`/`map_err`.

## Integration Points / Dependencies

- **No new crate dependency** — the core reuses store internals; server already depends on store.
- **No schema migration** — schema version stays 30 (weak mode, D-2). `previous_hash`/`version`
  columns already exist and already export (`export.rs:387-393`); round-trip needs no export change
  (SR-07 verified). The only round-trip work is a test proving a mixed legacy+populated corpus
  re-imports and re-validates clean (AC-05).
- **Correction path input assumption (A-1):** `original` (loaded at `write_ext.rs:462`) is
  expected to carry a populated `content_hash`. If an active entry ever has an empty
  `content_hash`, the successor inherits an empty `previous_hash`, which the core tolerates as
  legacy (degrades safely, no crash). Spec should assert `original.content_hash` non-empty at
  correction time as a defensive check.

## Explicitly Out of Scope (deferred — zero implementation, zero tests, follow-up only)

To keep architecture, spec, and risk-test strategy consistent (no unsatisfiable test on a branch
that does not exist):

1. **The enterprise admin MCP verify tool.** The core is *factored* to be wrappable, but **no MCP
   tool is built in v1** (D-4). Zero MCP-tool tests. Follow-up gated on RBAC.
2. **Strong cryptographic cascade** (`content_hash = H(title, content, previous_hash)`) — NON-GOAL,
   north-star (goal #5474). Zero tests. Prerequisite is the external HEAD anchor, not the digest.
3. **External HEAD anchor** — NON-GOAL. Zero tests.
4. **Legacy backfill migration** — NON-GOAL (forward-only, D-2). Zero tests; no migration v31.
5. **Background/maintenance-tick continuous verify** — deferred (D-3). Zero tests.

## Open Questions

1. **`query_all_entries` status coverage.** The verify core must see Deprecated (superseded)
   entries — they are the predecessors. Confirm `query_all_entries()` returns all statuses; if it
   filters to Active, the CLI loader must use an all-status query (or a new `query_all_entries`
   with no status filter). Flag for spec/dev; verify tests must include a Deprecated predecessor.
2. **`ChainReport` output format.** v1 is human-readable text naming entry ids and the break kind
   (SR-05). A machine-readable `--json` flag is a plausible near-term follow-up for the MCP wrapper
   but is **not** in v1 scope — leave to spec if the tester wants a stable assertion surface.
3. **File-size headroom.** `import/mod.rs` is near the 500-line ceiling; refactoring
   `validate_hashes` to call the store core should *reduce* it. Confirm the new `chain_verify.rs`
   and `verify.rs` each stay well under 500 lines (they will — the core is ~120 lines).
