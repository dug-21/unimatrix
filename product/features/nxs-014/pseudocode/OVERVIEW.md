# nxs-014 Pseudocode — OVERVIEW

Cross-version hash chain (weak mode) + transport-agnostic chain-verify core.
Component decomposition per ARCHITECTURE §Component Breakdown and IMPLEMENTATION-BRIEF §Component Map.

## Components (build order)

| # | Component | File | Crate / Location | New/Changed |
|---|-----------|------|------------------|-------------|
| 1 | chain-verify-core | chain-verify-core.md | `unimatrix-store/src/chain_verify.rs` (new) + `lib.rs` | New |
| 2 | correction-write-path | correction-write-path.md | `unimatrix-store/src/write_ext.rs` (2 sites + FR-04 guard) | Changed |
| 3 | import-validation | import-validation.md | `unimatrix-server/src/import/mod.rs` `validate_hashes` | Refactored |
| 4 | verify-cli | verify-cli.md | `unimatrix-server/src/verify.rs` (new) + `main.rs` + `lib.rs` | New |
| 5 | readme-integrity | readme-integrity.md | repo `README.md` | Changed |

**Sequencing constraint:** Component 1 (`verify_entries` + the `ChainReport`/`ChainViolation`/`ViolationKind`
types) is the dependency for Components 3 and 4 — build it first. Component 2 is independent (write path).
Component 5 is doc-only, same PR (C-08). Components 3 and 4 are the two thin callers of the single oracle
(ADR-001) and can be built in parallel once Component 1 lands.

## Shared types (defined ONCE in `unimatrix-store/src/chain_verify.rs`)

These are the integration surface; Components 3 and 4 consume them, never redefine them (C-07 — no
transport/CLI/MCP types in the core).

```
pub struct ChainReport {
    pub checked: usize,          // entries EXAMINED (content_hash recomputed) — every entry increments this once
    pub skipped_legacy: usize,   // entries whose CHAIN-LINK step was skipped (previous_hash == "")
    pub violations: Vec<ChainViolation>,
}
impl ChainReport {
    pub fn is_clean(&self) -> bool { self.violations.is_empty() }   // NFR-06: false whenever violations non-empty
    pub fn describe(&self) -> String { ... }                        // human-readable, names every offending id
}
impl Display for ChainReport { ... }                               // used by CLI summary + import Err message

pub struct ChainViolation { pub entry_id: u64, pub kind: ViolationKind }

pub enum ViolationKind {
    ContentHashMismatch { computed: String, stored: String },
    ChainLinkMismatch   { predecessor_id: u64, expected: String, found: String },
    MissingPredecessor  { predecessor_id: u64 },
    DanglingPreviousHash,
}
```

**Counter semantics (resolves the R-02 vs R-03 assertion tension — see Open Question 1):**
- `checked` increments **once per entry** at the top of the loop — it counts entries the core *examined*
  (content_hash was recomputed). It therefore ALWAYS equals `entries.len()`. This makes R-02's
  "the Deprecated predecessor is counted in `checked`" assertion hold (the genesis/Deprecated predecessor
  is examined like any other entry).
- `skipped_legacy` increments for entries whose **chain-link** step was skipped because `previous_hash == ""`.
  A legacy/genesis entry is counted in BOTH `checked` (its content was examined) AND `skipped_legacy` (its
  link step was skipped). This is intentional and satisfies R-03's "skipped_legacy counts every empty-hash
  entry" assertion. The two counters answer different questions and are not mutually exclusive.

## Existing interfaces reused (do NOT rename — from ARCHITECTURE §Integration Surface)

| Interface | Signature | Location | Notes |
|-----------|-----------|----------|-------|
| `compute_content_hash` | `pub fn compute_content_hash(title: &str, content: &str) -> String` | `store/src/hash.rs:7` | **FROZEN** (C-01) |
| `EntryRecord` | fields `content_hash`, `previous_hash`, `version: u32`, `supersedes: Option<u64>`, `id: u64`, `title`, `content`, `status` | `store/src/schema.rs:49` | existing |
| `entry_from_row` | `pub fn entry_from_row(row: &SqliteRow) -> Result<EntryRecord>` | `store/src/read.rs:20` | used by import loader |
| `ENTRY_COLUMNS` | `pub const ENTRY_COLUMNS: &str` (all cols, DDL order) | `store/src/read.rs:11` | used by import loader |
| `query_all_entries` | `pub async fn query_all_entries(&self) -> Result<Vec<EntryRecord>>` | `store/src/read.rs:324` | **verified all-status** — see below |
| `SqlxStore::open_readonly` | `pub async fn open_readonly(path) -> Result<SqlxStore>` | `store/src/db.rs:147` | CLI loader |
| `ensure_data_directory` | `fn ensure_data_directory(project_dir, base_dir) -> Result<ProjectPaths>` | `server/src/project` | yields `db_path` |

### R-02 loader status coverage — RESOLVED, no read.rs change needed

`query_all_entries` (read.rs:324-340) issues `SELECT {ENTRY_COLUMNS} FROM entries` with **no `WHERE status`
filter** — it already returns ALL statuses incl. `Deprecated` (GH #266). The CLI loader is therefore correct
as-is. The import loader currently selects only `id, title, content, content_hash, previous_hash` with no
status filter (all-status by construction) but an incomplete column set; Component 3 replaces it with a
full-row `SELECT {ENTRY_COLUMNS} FROM entries` (still no status filter). **Neither loader needs a status-filter
fix; both are all-status.** Pseudocode makes the all-status requirement explicit at each loader call so a
future edit cannot silently narrow it. Verify tests must still include a Deprecated predecessor and assert it
is counted (R-02).

## Data flow across boundaries

```
CORRECTION (write, store)
  correct_entry(original_id, correction, ...)
    original = entry_from_row(read original_id)        // write_ext.rs:462 (already loaded)
    GUARD (FR-04): original.content_hash empty -> Err(InvalidInput naming original_id); persist nothing
    new_rec.previous_hash = original.content_hash       // struct :539  (was String::new())
    new_rec.version       = original.version + 1        // struct :540  (was 1)
    INSERT ... .bind(&new_rec.previous_hash)            // bind  :582  (was .bind(""))
               .bind(new_rec.version as i64)            // bind  :583  (was .bind(1_i64))

VERIFY (read, two callers, ONE pure core)
  CLI:    run_verify(project_dir)
            paths   = ensure_data_directory(project_dir, None)
            store   = SqlxStore::open_readonly(paths.db_path)
            entries = store.query_all_entries()          // ALL statuses (incl. Deprecated predecessors)
            report  = chain_verify::verify_entries(&entries)   // PURE CORE — no I/O
            print report; exit 0 if is_clean() else non-zero

  import: validate_hashes(&mut conn)                     // inside BEGIN IMMEDIATE, pre-COMMIT
            rows    = SELECT {ENTRY_COLUMNS} FROM entries  (on `conn`, ALL statuses, uncommitted rows)
            entries = rows.map(entry_from_row)
            report  = chain_verify::verify_entries(&entries)   // SAME PURE CORE
            if !report.is_clean() { return Err(report.describe()) }  // caller ROLLBACKs before COMMIT
```

The core touches no I/O; each caller supplies the entry slice from its own connection (import from its
in-flight transaction so it sees uncommitted rows; CLI from a read-only pool). This is the transport-agnostic
seam D-4 mandates (a future MCP wrapper hands the core entries it already holds — zero new logic).

## Cross-cutting constraints applied in every component

- C-01 `compute_content_hash` FROZEN — recompute only, never re-derive/fold `previous_hash`.
- C-02/FR-06 empty `previous_hash` == unverifiable-legacy skip, never a break (matches import/mod.rs:429).
- C-03/R-01 correction changes BOTH literal sites, binding from the record.
- C-05/NFR-02 no schema migration; schema stays 30.
- C-06/NFR-03 max 500 lines/file — `chain_verify.rs` ~120 lines, `verify.rs` well under; refactor shrinks `import/mod.rs`.
- C-07 core signature free of CLI/MCP types.
- NFR-05 O(entries) single pass, in-memory `HashMap<u64,&EntryRecord>`; no quadratic re-walk, no recursion.
- NFR-06 fail-loud on every surface; `is_clean()==false` ⇒ CLI non-zero exit AND import `Err`.

## Open questions / gaps (flagged, not silently resolved)

1. **`checked` counter semantics (R-02 vs R-03 tension).** Resolved above: `checked` = entries examined
   (== `entries.len()`), `skipped_legacy` = link-step skips; a legacy entry is in both. Tester should assert
   `report.checked == corpus.len()` and `report.skipped_legacy == count_of_empty_previous_hash`. Flagging so
   implementer and tester adopt the SAME definition; if the tester wants `checked` to EXCLUDE legacy, that
   contradicts R-02 scenario 1 and must be reconciled at Gate 3a.
2. **`ChainReport` output format is human-readable text only in v1** (SR-05); no `--json` flag (Open Q2 in
   ARCHITECTURE — out of v1 scope). `describe()`/`Display` must name each offending `entry_id` + kind.
