# nxs-014 — Implementation Brief: Wire the Cross-Version Hash Chain in `context_correct` (Weak Mode)

> Compiled from the SETTLED Session 1 design artifacts (SCOPE D-1..D-4, ADR-001..003,
> SPECIFICATION FR-01..FR-11 / AC-01..AC-12, RISK-TEST-STRATEGY R-01..R-12, ALIGNMENT-REPORT).
> This is the single entry point for Session 2 delivery agents.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nxs-014/SCOPE.md |
| Scope Risk Assessment | product/features/nxs-014/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nxs-014/architecture/ARCHITECTURE.md |
| ADR-001 (verify-core placement) | product/features/nxs-014/architecture/ADR-001-chain-verify-core-placement.md |
| ADR-002 (weak-mode threat boundary) | product/features/nxs-014/architecture/ADR-002-weak-mode-threat-boundary.md |
| ADR-003 (chain population + verify semantics) | product/features/nxs-014/architecture/ADR-003-correction-chain-population-and-verify-semantics.md |
| Specification | product/features/nxs-014/specification/SPECIFICATION.md |
| Risk / Test Strategy | product/features/nxs-014/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nxs-014/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/nxs-014/ACCEPTANCE-MAP.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| chain-verify-core (`unimatrix-store/src/chain_verify.rs`, new) | pseudocode/chain-verify-core.md | test-plan/chain-verify-core.md |
| correction-write-path (`unimatrix-store/src/write_ext.rs`, changed) | pseudocode/correction-write-path.md | test-plan/correction-write-path.md |
| import-validation (`unimatrix-server/src/import/mod.rs` `validate_hashes`, refactored) | pseudocode/import-validation.md | test-plan/import-validation.md |
| verify-cli (`unimatrix-server/src/verify.rs` + `main.rs`, new) | pseudocode/verify-cli.md | test-plan/verify-cli.md |
| readme-integrity (`README.md`, changed) | pseudocode/readme-integrity.md | test-plan/readme-integrity.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode/ and test-plan/ files are produced in Session 2 Stage 3a. Paths above are the
expected component decomposition from the architecture — actual files are filled during delivery.

## Goal

`context_correct` hardcodes `previous_hash = ""` and `version = 1` on every correction, so the
cross-version chain column defined in the schema is never populated — violating Architectural
Principle 1 and leaving the README's "tamper-evident" claim unbacked (GH #912). This feature wires
**weak-mode** chain population (set `previous_hash` from the superseded entry's `content_hash` and
increment `version`) at both the struct and INSERT-bind sites, adds a **transport-agnostic
chain-verify core** in `unimatrix-store` exposed via a new `verify` CLI subcommand and reused by
import, and corrects the README from "tamper-evident" to the truthful "tamper-**recorded**" scope.
No hash-format change, no cascade, no anchor, no migration.

## Resolved Decisions

| Decision | Resolution | Source | ADR |
|----------|-----------|--------|-----|
| D-1 — threat model / feature size | **WEAK MODE.** Populate `previous_hash = superseded.content_hash`, `version = superseded.version + 1`. No cascade digest change, no anchor, no re-hash migration. Strong chain + head anchor = NON-GOAL / north-star. README corrected to tamper-**RECORDED**. | SCOPE D-1; SR-01, SR-04 | ADR-002 |
| D-2 — legacy backfill | **FORWARD-ONLY.** Existing corrected entries keep empty `previous_hash`; verify tolerates empty as unverifiable-legacy. No migration; schema stays v30. | SCOPE D-2; SR-02 | ADR-003 |
| D-3 — verify cadence | Correction-time link invariant (free) **+** full chain-verify at import **+** on-demand CLI check. Defer maintenance-tick/periodic scan. | SCOPE D-3 | ADR-001, ADR-003 |
| D-4 — verify exposure | CLI subcommand + server-internal core in v1; **no MCP tool**. Verify core is transport-agnostic so a future post-RBAC MCP tool is a thin wrapper. | SCOPE D-4; SR-03 | ADR-001 |
| Verify-core crate home (SR-03 open decision) | **`unimatrix-store`** (leaf crate) as a pure I/O-free `verify_entries(&[EntryRecord]) -> ChainReport`; `validate_hashes` + CLI are thin callers. No dependency cycle; one oracle. | ARCHITECTURE §Why the verify core lives in store | ADR-001 |
| Hash format | **FROZEN.** `compute_content_hash(title, content)` signature and output unchanged. Folding `previous_hash` into the digest is out of scope and must fail review. | SCOPE Constraints; NFR-01 | ADR-002 |

## Delivery-Critical Carry-Items (read before writing code)

These are the ways this feature ships broken. Each has a non-negotiable test.

1. **R-01 / SR-06 (Critical) — two-site half-fix.** `write_ext.rs` hardcodes the link at TWO
   independent places: the `EntryRecord` struct literal (`:539-540`) AND the INSERT binds
   (`:582-583`, `.bind("")` / `.bind(1_i64)` — inline, NOT reading the record fields). Fixing only
   the struct compiles clean and STILL persists empty. **Both sites change.** The acceptance test
   (AC-01/AC-02) MUST read `previous_hash`/`version` **back from the DB** via a fresh query, never
   from the in-memory returned record — a struct-only half-fix passes an in-memory assertion green.
2. **R-02 (Critical) — Deprecated predecessor visibility (architect Open Q1).** Predecessors are
   `Deprecated` (superseded). If `query_all_entries()` (CLI loader) OR the import loader filters to
   `Active`, every chained successor's `supersedes` target is absent from the map → `MissingPredecessor`
   false-alarms on a perfectly clean corpus. **Confirm both loaders return Deprecated entries.**
   Verify tests MUST include a Deprecated predecessor and assert it is counted as `checked`.
3. **R-03 / SR-02 — forward-only legacy tolerance.** The chain check MUST skip empty
   `previous_hash` as unverifiable-legacy (matching `import/mod.rs:429`), NOT a break. A mixed
   legacy(empty)+new(chained) corpus MUST verify clean.
4. **R-04 — `validate_hashes` refactor is behavior-changing.** The core's link check keys on the
   authoritative `supersedes` edge (`successor.previous_hash == predecessor.content_hash`), strictly
   stronger than the old "references *some* known hash" existence check. **Existing import tests are
   the regression tripwire** — run them unchanged first; any diff must be justified by the
   stronger check, not by loosened tolerance.
5. **AC-06/AC-07 — Honest-README correction ships in the SAME PR** as the code. Remove the
   unqualified "tamper-evident" correction-chain claim; state the tamper-**recorded** guarantee.
   Do not under-sell shipped integrity (content_hash, append-only audit, supersession chain).

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/chain_verify.rs` | **create** | Pure I/O-free verify core: `verify_entries(&[EntryRecord]) -> ChainReport`; content-hash recompute + supersedes-link check, skip empty legacy. ~120 lines. |
| `crates/unimatrix-store/src/write_ext.rs` | modify | Correction path: set `previous_hash`/`version` from loaded `original` at BOTH struct (`:539-540`) and INSERT binds (`:582-583`); reject empty `original.content_hash` (FR-04). |
| `crates/unimatrix-store/src/lib.rs` (or module root) | modify | Register/`pub` the new `chain_verify` module. |
| `crates/unimatrix-server/src/import/mod.rs` | modify | Refactor `validate_hashes` into a thin adapter over `verify_entries` (loads entries from in-flight transaction connection). |
| `crates/unimatrix-server/src/verify.rs` | **create** | `run_verify(project_dir) -> Result<(), Box<dyn Error>>`: resolve db path, open read-only, `query_all_entries`, call core, print report, `Err` on non-clean. |
| `crates/unimatrix-server/src/main.rs` | modify | Add `Command::Verify { }` variant, dispatch in the pre-Tokio sync block (mirrors `Command::Import`). |
| `README.md` | modify | Rewrite integrity section to tamper-**recorded** boundary (ADR-002). Same PR. |

Verify `query_all_entries` (`read.rs:324`) returns ALL statuses incl. Deprecated (R-02); if it
filters to Active, add/use an all-status query for both the CLI loader and the import loader.

## Data Structures (new, in `chain_verify.rs`)

```rust
pub struct ChainReport {
    pub checked: usize,
    pub skipped_legacy: usize,
    pub violations: Vec<ChainViolation>,
}
impl ChainReport { pub fn is_clean(&self) -> bool { self.violations.is_empty() } }
// + Display / describe()

pub struct ChainViolation { pub entry_id: u64, pub kind: ViolationKind }

pub enum ViolationKind {
    ContentHashMismatch { computed: String, stored: String },
    ChainLinkMismatch { predecessor_id: u64, expected: String, found: String },
    MissingPredecessor { predecessor_id: u64 },
    DanglingPreviousHash,
}
```

## Function Signatures (integration surface — do not rename)

| Interface | Signature | Location |
|-----------|-----------|----------|
| `compute_content_hash` — **FROZEN** | `pub fn compute_content_hash(title: &str, content: &str) -> String` | `unimatrix-store/src/hash.rs:7` |
| `verify_entries` — **NEW** (pure core) | `pub fn verify_entries(entries: &[EntryRecord]) -> ChainReport` | `unimatrix-store/src/chain_verify.rs` |
| `entry_from_row` | `pub fn entry_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<EntryRecord>` | `unimatrix-store/src/read.rs:20` |
| `ENTRY_COLUMNS` | `pub const ENTRY_COLUMNS: &str` | `unimatrix-store/src/read.rs:11` |
| `query_all_entries` — **must return ALL statuses** | `pub async fn query_all_entries(&self) -> Result<Vec<EntryRecord>>` | `unimatrix-store/src/read.rs:324` |
| `SqlxStore::open_readonly` | `pub async fn open_readonly(path: impl AsRef<Path>) -> Result<SqlxStore>` | `unimatrix-store/src/db.rs:147` |
| `ensure_data_directory` | `fn ensure_data_directory(project_dir: Option<&Path>, base_dir: Option<&Path>) -> Result<ProjectPaths>` | `unimatrix-server/src/project` |
| `run_verify` — **NEW** | `pub fn run_verify(project_dir: Option<&Path>) -> Result<(), Box<dyn std::error::Error>>` | `unimatrix-server/src/verify.rs` |
| CLI variant — **NEW** | `Command::Verify { }` dispatched in `fn main()` pre-Tokio sync block | `unimatrix-server/src/main.rs` |

### `EntryRecord` chaining fields (existing)

`content_hash: String`, `previous_hash: String`, `version: u32`, `supersedes: Option<u64>`,
`superseded_by: Option<u64>`, `id: u64`, `status: Status` (`unimatrix-store/src/schema.rs:49`).

## Verify Algorithm (corpus-wide, O(n) — from ADR-003)

Build `HashMap<u64, &EntryRecord>` keyed by `id`. For each entry:

1. **Content hash:** recompute `compute_content_hash(title, content)`; if `!= content_hash` →
   `ContentHashMismatch`. (Catches naive content mutation — AC-04.)
2. **Chain link:** if `previous_hash` empty → skip as unverifiable-legacy, `skipped_legacy += 1`
   (SR-02, AC-03). Else resolve predecessor via the authoritative `supersedes` edge:
   - `supersedes == None` → `DanglingPreviousHash` (fail loud, do not ignore).
   - predecessor id not in map → `MissingPredecessor { predecessor_id }`.
   - `predecessor.content_hash != previous_hash` → `ChainLinkMismatch { predecessor_id, expected, found }`.

Report names every offending entry id + break kind (fail-loud). Link check keyed on `supersedes` is
strictly stronger than and subsumes the old "references some known hash" existence check (R-04).

## Data Flow (from ARCHITECTURE)

```
correction: store::write_ext::correct_entry
  original = read(original_id)                  // already loaded at write_ext.rs:462
  new_rec.previous_hash = original.content_hash  // struct :539
  new_rec.version       = original.version + 1   // struct :540
  INSERT ... .bind(&new_rec.previous_hash)        // bind :582
             .bind(new_rec.version as i64)        // bind :583

verify (two callers, one pure core):
  CLI:    verify::run_verify(project_dir) -> open_readonly -> query_all_entries (ALL statuses)
                                          -> verify_entries(&entries) -> print + exit code
  import: validate_hashes(&mut conn) -> load all rows on in-flight txn conn (ENTRY_COLUMNS
                                          + entry_from_row) -> verify_entries(&entries)
                                          -> non-clean => ROLLBACK + Err
```

Import must load entries from its in-flight `BEGIN IMMEDIATE` connection so it sees uncommitted rows.

## Constraints

- **C-01:** `compute_content_hash` FROZEN — no signature or output change; no folding `previous_hash`
  into the digest (strong mode / Non-Goal 1). `hash.rs` known-value vectors (`e3b0c442…` genesis,
  `"Test: Content"`) are the tripwire (SR-01, AC-10).
- **C-02:** Chain-verify MUST skip empty `previous_hash` as unverifiable-legacy, matching
  `import/mod.rs:429` (SR-02, A-3).
- **C-03:** BOTH the struct literal (`:539-540`) and INSERT bind (`:582-583`) change; either alone
  is a half-fix (SR-06).
- **C-04:** Non-negotiable tests read `previous_hash`/`version` **back from the DB** after
  correction; in-memory-only assertions are false-green (SR-06, AC-01/02).
- **C-05:** No schema migration; schema version stays 30 (NFR-02).
- **C-06:** Max 500 lines/file (rust-workspace rule); `chain_verify.rs` and `verify.rs` each stay
  well under. Refactor should shrink `import/mod.rs` (near the ceiling).
- **C-07:** Verify core is transport-agnostic — no CLI/MCP types in its signature (D-4, SR-03).
- **C-08:** README correction ships in the SAME PR as the code (AC-06).
- **C-09:** Chain-verify trusts the `supersedes`/`superseded_by` chain (A-2); it verifies hashes
  along that chain, not the topology.
- **NFR-06:** Fail-loud, never warn-and-continue; `is_clean()` false whenever `violations` non-empty.

## Dependencies

- **Crates/components:** `unimatrix-store` (`write_ext.rs`, `hash.rs`, `schema.rs::EntryRecord`,
  `read.rs::entry_from_row`/`ENTRY_COLUMNS`/`query_all_entries`, `db.rs::open_readonly`, new
  `chain_verify.rs`); `unimatrix-server` (`import/mod.rs::validate_hashes`, `export.rs`, `main.rs`
  clap `Command`, new `verify.rs`, `project::ensure_data_directory`).
- **External:** `sqlx` (SQLite), `sha2` — both existing. **No new crate dependency.**
- **Existing conventions relied on:** `validate_hashes` empty-`previous_hash` tolerance (`:429`);
  `Export`/`Import` direct-DB, no-server pattern for the new CLI subcommand.

## NOT in Scope

- **Strong cryptographic cascade** (`content_hash = H(title, content, previous_hash)`) — hash-format
  change; NON-GOAL / north-star `KI-CHAIN-XV-STRONG`. Zero tests.
- **External HEAD anchor** (signed/published/out-of-DB append-only log) — NON-GOAL. Zero tests.
- **Defending against a root / raw-DB-write adversary** — out of tier. Do NOT write a test asserting
  detection of a perfectly-coordinated multi-row rewrite (documented limitation, not a test gap).
- **Legacy backfill migration** — forward-only; no migration v31 (D-2). Zero tests.
- **Continuous background/maintenance-tick verify** — import + on-demand only (D-3). Zero tests.
- **MCP tool for chain-verify** — no v1 MCP tool; CLI only (D-4, FR-09). Zero MCP-tool tests.
- **Changing supersession semantics** — already correct (Non-Goal 6).

## Alignment Status

ALIGNMENT-REPORT: all six checks PASS (Vision, Milestone, Scope Gaps, Scope Additions, Architecture,
Risk). Weak mode moves `context_correct` from *violating* Architectural Principle 1 to satisfying it
literally and advances the Knowledge Integrity goal's (#5474) north-star leg. One additive behavior
(FR-04, reject correction on empty `original.content_hash`) is defensive and traced to A-1.

**One variance — V-1 (knowledge-governance, NON-blocking for code):** Capability #5478 (KI-CHAIN-XV)
is worded as tamper-**EVIDENCE**, but weak mode delivers tamper-**RECORDED**. #5478's `done_when` IS
fully satisfied by weak mode, but its name/why overclaim. **Before #5478 is marked `proven`,** either
re-word it to tamper-RECORDED and split a `KI-CHAIN-XV-STRONG` sibling (carrying the tamper-EVIDENT
promise onto the north-star), or mark it `proven` only against a re-scoped tamper-RECORDED `done_when`.
This is a vision-session action, **NOT a code gate** — delivery proceeds; the `proven` marking waits.

## Knowledge Stewardship

- Queried: read-only synthesis of the SETTLED Session 1 artifacts (SCOPE, SCOPE-RISK-ASSESSMENT,
  ARCHITECTURE, ADR-001..003, SPECIFICATION, RISK-TEST-STRATEGY, ALIGNMENT-REPORT). No Unimatrix
  query needed — this agent compiles existing artifacts into delivery deliverables.
- Stored: nothing. Synthesizer is storage-exempt; ADRs already live in files per this feature's
  architecture. V-1 capability-wording reconciliation is carried to the vision session, not stored.
