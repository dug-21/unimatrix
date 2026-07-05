# nxs-014 — Specification: Wire the Cross-Version Hash Chain (WEAK MODE)

> Source: `product/features/nxs-014/SCOPE.md` (SETTLED, D-1..D-4) and
> `product/features/nxs-014/SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-07, A-1..A-3).
> Threat model, mode selection, and cadence are settled decisions — this spec does not reopen them.

## Objective

`context_correct` currently hardcodes `previous_hash = ""` and `version = 1` on every correction,
so the cross-version chain column defined in the schema is never populated — violating Architectural
Principle 1 ("`content_hash` and `previous_hash` on every entry — never skipped") and leaving the
README's integrity claim unbacked. This feature wires **weak-mode** chain population (populate
`previous_hash` from the superseded entry's `content_hash` and increment `version`) at both the struct
and the SQL INSERT-bind site, adds a **transport-agnostic chain-verify core** exposed today via a CLI
subcommand (future MCP wrapper deferred), and corrects the README wording from "tamper-evident" to the
truthful "tamper-**recorded** / correction-history integrity" scope. No hash-format change, no cascade,
no anchor, no migration.

## Domain Models & Ubiquitous Language

| Term | Definition |
|------|------------|
| **Correction** | A `context_correct` operation: the original active entry is deprecated (`superseded_by = new_id`, `status = Deprecated`), and a new entry is written with `supersedes = original_id`. Path: `unimatrix-store/src/write_ext.rs:439-620`. |
| **`content_hash`** | `SHA256(format!("{title}: {content}"))`, engine-computed in the write path from the entry's own title+content, never caller-supplied (ADR-004, entry #74). **Signature and output format are FROZEN** in this feature. |
| **`previous_hash`** | The chain-link column. In weak mode, on a correction it equals the **superseded (predecessor) entry's `content_hash`**. Empty string (`""`) means "genesis or forward-only legacy — unverifiable, not broken." |
| **`version`** | Monotonic per-chain counter. Genesis entry = 1; each correction = `predecessor.version + 1`. |
| **Supersedes chain** | The linked list formed by `supersedes` / `superseded_by`. `predecessor` = the entry pointed to by a successor's `supersedes`. Assumed already correct (A-2, Non-Goal 6). |
| **Chain-verify core** | A transport-agnostic function that walks the supersedes chain(s) over a live DB and, per hop with a non-empty `previous_hash`, asserts `successor.previous_hash == predecessor.content_hash`; it also recomputes `content_hash` from title+content and compares. Fails loud, naming the offending entry id. |
| **Tamper-recorded (not tamper-evident)** | The delivered guarantee: accidental corruption and single-point API-surface tampering are **detectable**; a root/DB-write adversary who coordinates edits across an entry and its successor is **out of tier** and NOT defended against. |
| **Forward-only legacy** | Pre-existing corrected entries retain `previous_hash = ""`. No backfill (D-2). Verify must treat these as unverifiable-legacy, never as a break. |

## Functional Requirements

Each FR is testable; verification methods are consolidated in Acceptance Criteria (traced by ID).

- **FR-01 — Populate link on correction (struct site).** In the correction path
  (`write_ext.rs`), the new `EntryRecord` struct MUST set `previous_hash = original.content_hash`
  (currently `String::new()` at `:539`) and `version = original.version + 1` (currently `1` at `:540`).
  `original` is already loaded at `:462`. (Traces AC-01, AC-02; SR-06.)

- **FR-02 — Populate link on correction (INSERT-bind site).** The INSERT statement MUST bind
  `previous_hash` and `version` **from the record fields** (`new_rec.previous_hash`, `new_rec.version`),
  replacing the hardcoded inline `.bind("")` (`:582`) and `.bind(1_i64)` (`:583`). This site is
  independent of FR-01; fixing only FR-01 compiles clean but still persists empty (the SR-06
  two-literal half-fix). Both sites are non-negotiable. (Traces AC-01, AC-02; SR-06.)

- **FR-03 — Version monotonicity across a chain.** For an N-step correction chain, the persisted
  `version` values MUST be `1, 2, …, N` in supersession order, and each hop's `previous_hash` MUST
  equal the immediately-preceding entry's `content_hash`. (Traces AC-02.)

- **FR-04 — Assert predecessor hash present at correction time.** Before writing the link, the
  correction path MUST treat an empty/absent `original.content_hash` as an error condition rather than
  silently writing an empty or bad `previous_hash` (A-1). Behaviour: fail the correction with a clear
  error naming `original_id`, rather than persist a malformed link. (Traces AC-08; A-1.)

- **FR-05 — Chain-verify core (transport-agnostic).** A verify core function MUST exist that, given a
  live DB connection/handle, walks supersedes chains corpus-wide and, for every hop whose successor has
  a **non-empty** `previous_hash`, asserts `successor.previous_hash == predecessor.content_hash`. It MUST
  additionally recompute each entry's `content_hash` from its title+content and compare to the stored
  value. On any inconsistency it MUST fail loud, and the failure MUST name the specific offending entry
  id(s) and the nature of the break (content-hash mismatch vs chain-link mismatch). The core takes no
  transport/CLI/MCP types in its signature so a future MCP admin tool is a thin wrapper (D-4, SR-03).
  (Traces AC-03, AC-04, AC-09.)

- **FR-06 — Forward-only legacy tolerance.** The chain-verify MUST treat an empty `previous_hash`
  (`""`) as **unverifiable-legacy / genesis**, NOT as a break — matching the existing convention at
  `import/mod.rs:429`. A corpus mixing legacy (empty `previous_hash`) and new (chained) corrected
  entries MUST verify clean. (Traces AC-03, AC-05; SR-02, A-3.)

- **FR-07 — Extend import validation with the chain check.** The import-time verifier
  (`validate_hashes`, `import/mod.rs:396`, run at `:212`) MUST invoke / share the same chain-verify
  logic (FR-05) so a corrected chain re-imports clean and a tampered one fails at import. The two
  existing behaviours (content-hash recompute at `:421`; non-empty-`previous_hash` existence check at
  `:429`) MUST be preserved. (Traces AC-05, AC-04.)

- **FR-08 — CLI subcommand invokes the verify core on the live DB.** A new subcommand on the
  `unimatrix` binary (`Command` enum, `main.rs:162`) MUST run the chain-verify core against the live
  project database (direct DB read, no running server required, consistent with `Export`/`Import`).
  Contract:
  - It scans all supersedes chains in the DB.
  - Exit code `0` when the corpus verifies clean; a non-zero exit code on any detected break.
  - On a break, human-readable output names the offending entry id(s) and the break type; on success,
    a concise summary (e.g., entries/chains checked).
  (Traces AC-09; SR-05, D-4.)

- **FR-09 — No MCP tool in v1.** No new MCP tool is added for chain-verify in this feature. The verify
  core is factored so a post-RBAC MCP wrapper can call it later, but that wrapper is out of scope.
  (Traces AC-11; D-4, Non-Goals.)

- **FR-10 — Export/import round-trip of `previous_hash`/`version`.** `previous_hash` and `version` MUST
  round-trip losslessly through export → import unchanged. The columns are already serialized on export
  (`export.rs:310-311`); this FR requires that a multi-hop corrected chain (including a legacy
  empty-hash entry) survives export→import with identical values and passes import chain-verify.
  (Traces AC-05; SR-07.)

- **FR-11 — Honest README wording.** In the same PR, the README integrity section MUST be corrected:
  remove/qualify any claim of "tamper-evident" chaining against a database-write adversary; state the
  actual delivered guarantee (accidental-corruption + single-point API-surface tamper detection =
  tamper-**recorded** / correction-history integrity). It MUST NOT under-sell shipped integrity
  (per-entry content_hash, append-only audit, authoritative supersession chain remain true). The claim
  boundary is fixed by D-1/SR-04. (Traces AC-06, AC-07.)

## Non-Functional Requirements

- **NFR-01 — Frozen hash format (hard constraint).** `compute_content_hash` signature and output MUST
  NOT change. No folding of `previous_hash` into `content_hash` (that is strong mode / Non-Goal 1).
  Verification: the `hash.rs` known-value test vectors (genesis `e3b0c442…`, `"Test: Content"` vectors)
  MUST remain unchanged and pass, acting as a tripwire against inline scope-creep (SR-01).
  (Traces AC-10.)

- **NFR-02 — No schema migration.** Weak mode requires no DDL change; schema version stays 30. Any
  introduction of migration v31 is out of scope. Verification: no change under `migration.rs` schema
  version, no new migration step. (Traces AC-12.)

- **NFR-03 — File size / module boundary.** Max 500 lines/file (rust-workspace rule). If the
  chain-verify core would bloat `import/mod.rs` past the limit, it MUST live in its own module. The
  crate home of the shared verify core (store vs server) is an architect decision (SR-03) — this spec
  requires only that CLI (today) and a future server-side MCP wrapper can both depend on it without a
  dependency cycle.

- **NFR-04 — Correction-time cost.** Populating the link is a write-time invariant (assigning two
  fields already in hand), not a scan. It MUST NOT add a corpus walk to the correction hot path.

- **NFR-05 — Verify performance is O(entries).** The on-demand/import chain-verify is a single linear
  pass over entries (hash lookups via an in-memory map, as `validate_hashes` already does). No
  quadratic chain re-walking.

- **NFR-06 — Fail-loud, not fail-silent.** Verify surfaces MUST return an error/non-zero exit that
  names the break; they MUST NOT warn-and-continue or return green on a detected inconsistency.

## Acceptance Criteria

Each AC lists a verification method. AC-01/AC-02 read **back from the DB** after correction (false-green
guard, SR-06) — never from the in-memory returned record.

- **AC-01 — Link populated (DB read-back).** *Method:* perform a correction, then issue a fresh SQL
  read of the new entry's row from the database; assert the persisted `previous_hash` equals the
  superseded entry's `content_hash`. Asserting on the in-memory `EntryRecord` alone is INSUFFICIENT and
  does not satisfy this AC (a struct-only fix leaves the INSERT binding `""`). (FR-01, FR-02; SR-06.)

- **AC-02 — Version increment (DB read-back).** *Method:* after a correction, read the new entry's
  `version` from the DB and assert it equals `superseded.version + 1`. Across an N-step chain, read all
  rows and assert versions are `1..N` monotonic in supersession order. DB read-back required. (FR-01,
  FR-02, FR-03.)

- **AC-03 — Chain-verify walks and links.** *Method:* build a multi-hop corrected chain; run the
  chain-verify core; assert it passes and that, for every hop with a non-empty `previous_hash`, the
  check `successor.previous_hash == predecessor.content_hash` was exercised. Empty `previous_hash`
  (genesis/legacy) is treated as unverifiable-legacy, not a break. (FR-05, FR-06.)

- **AC-04 — Tamper fails loud, names the entry.** *Method:* in a verified corpus, mutate a superseded
  entry's `content` directly in the DB **without** perfectly rewriting both its `content_hash` and its
  successor's `previous_hash`; run chain-verify; assert it fails and the error message names the
  offending entry id. This AC is satisfied by the **combination** of the content-hash recompute and the
  chain-link check — both must run. (FR-05, FR-07.)

- **AC-05 — Mixed legacy+new round-trip verifies clean.** *Method:* construct a corpus mixing legacy
  entries (`previous_hash = ""`) and new chained corrected entries; verify it passes; export → import
  it; assert `previous_hash`/`version` are unchanged after re-import and that import-time chain-verify
  passes on the clean re-import. Then, as a paired negative, mutate a superseded entry's content and
  assert verify fails loud. (FR-06, FR-07, FR-10; SR-02, SR-07.)

- **AC-06 — README wording corrected.** *Method:* inspect the README integrity section in the PR diff;
  assert no unqualified "tamper-evident"/"tamper evident" claim about the correction chain against a
  DB-write adversary remains, and that the corrected text states the tamper-recorded /
  correction-history-integrity guarantee. Same PR as the code change. (FR-11; D-1, SR-04.)

- **AC-07 — Threat model documented durably.** *Method:* the threat model (what is / is not defended:
  accidental corruption + API-surface tamper detectable; DB-write adversary out of tier) is recorded in
  a durable location — the README integrity section and/or an ADR — so downstream agents do not
  re-upgrade the claim. (FR-11; SR-04.)

- **AC-08 — Empty predecessor hash rejected at correction.** *Method:* attempt a correction whose
  `original` has an empty `content_hash`; assert the correction fails with an error naming `original_id`
  rather than persisting an empty/malformed `previous_hash`. (FR-04; A-1.)

- **AC-09 — CLI verify contract.** *Method:* run the new CLI subcommand against (a) a clean corpus —
  assert exit code 0 and a summary of what was checked; (b) a tampered corpus — assert non-zero exit
  code and output naming the offending entry id. (FR-08; SR-05.)

- **AC-10 — Hash format frozen (tripwire).** *Method:* the `hash.rs` known-value test vectors remain
  byte-identical and pass; `compute_content_hash` signature is unchanged. Any diff to the hash format
  fails this AC. (NFR-01; SR-01.)

- **AC-11 — No MCP tool added.** *Method:* assert no new MCP tool for chain-verify is registered in the
  server tool surface; the verify core signature is free of transport/CLI/MCP types. (FR-09; D-4.)

- **AC-12 — No migration.** *Method:* assert schema version is unchanged (still 30) and no new
  migration step is added. (NFR-02.)

## User / Agent Workflows

1. **Correction (existing surface, now correct).** A caller invokes `context_correct`. The engine
   deprecates the original, computes the new entry's `content_hash`, sets `previous_hash` to the
   original's `content_hash` and `version` to `original.version + 1`, and persists both at the struct
   and INSERT sites. No caller-visible API change.
2. **On-demand integrity check (new CLI).** An operator runs the new `unimatrix` subcommand against a
   project DB. It walks all supersedes chains, prints a summary, and exits 0 (clean) or non-zero
   (naming the break).
3. **Import verification (existing path, strengthened).** On `import`, `validate_hashes` runs the shared
   chain-verify; a corrected export re-imports clean, a tampered one fails at import.
4. **Future (out of scope).** A post-RBAC enterprise admin MCP tool wraps the same verify core — not
   built here.

## Constraints

- **C-01:** `compute_content_hash` is FROZEN — no signature or output change (NFR-01, boundary between
  weak/strong). Inline "improvement" to fold `previous_hash` in is prohibited (SR-01).
- **C-02:** Chain-verify MUST skip empty `previous_hash` as unverifiable-legacy, matching
  `import/mod.rs:429` (SR-02, A-3).
- **C-03:** BOTH the struct literal (`write_ext.rs:539-540`) and the INSERT bind (`:582-583`) change;
  either alone is a half-fix (SR-06).
- **C-04:** Non-negotiable tests read `previous_hash`/`version` **back from the DB** after correction;
  in-memory-only assertions are false-green (SR-06, AC-01/AC-02).
- **C-05:** No schema migration; schema version stays 30 (NFR-02).
- **C-06:** Max 500 lines/file; split the verify core into a module if needed (NFR-03).
- **C-07:** Verify core is transport-agnostic — no CLI/MCP types in its signature (D-4, SR-03).
- **C-08:** README correction ships in the SAME PR as the code (AC-06).
- **C-09:** Chain-verify trusts the `supersedes`/`superseded_by` chain as correct (A-2); it verifies
  hashes along that chain, not the chain topology itself.

## Dependencies

- **Crates/components:** `unimatrix-store` (`write_ext.rs` correction path, `hash.rs`,
  `schema.rs::EntryRecord`, `read.rs::entry_from_row`); `unimatrix-server` (`import/mod.rs::validate_hashes`,
  `export.rs`, `main.rs` `Command` enum / clap); `sqlx` (SQLite); `sha2` (existing).
- **Existing conventions relied on:** `validate_hashes` empty-`previous_hash` tolerance (`:429`);
  `Export`/`Import` direct-DB, no-server pattern for the new CLI subcommand.
- **Open architect decision (not resolved here):** cross-crate home of the shared verify core
  (SR-03) — likely `unimatrix-store` with server re-export to avoid a dependency cycle; the architect
  fixes this in an ADR before pseudocode.

## NOT in Scope (explicit exclusions)

- **Strong cryptographic cascade** — `content_hash = H(title, content, previous_hash)`. Hash-format
  change; breaks known-value vectors and collides every legacy `content_hash`. NON-GOAL (Non-Goal 1,
  SR-01). North-star `KI-CHAIN-XV-STRONG`.
- **External HEAD anchor** (signed / published / out-of-DB append-only log) — the real prerequisite for
  tamper-evidence against a DB-write adversary. Separate feature (Non-Goal 2).
- **Defending against a root / raw-DB-write adversary** — out of tier (Non-Goal 3).
- **Legacy backfill migration** (populating `previous_hash` on pre-existing corrected entries) —
  forward-only; no migration v31 (D-2, Non-Goal 4).
- **Continuous background chain-verify on the maintenance tick** — import + on-demand only for v1
  (D-3, Non-Goal 5).
- **MCP tool for chain-verify** — no v1 MCP tool; CLI only (D-4, FR-09).
- **Changing supersession semantics** (`supersedes`/`superseded_by`) — already correct (Non-Goal 6).

## Traceability Summary

| AC | Source | Risk / Assumption |
|----|--------|-------------------|
| AC-01 | SCOPE AC-01; FR-01/02 | SR-06 |
| AC-02 | SCOPE AC-02; FR-01/02/03 | SR-06 |
| AC-03 | SCOPE AC-03; FR-05/06 | SR-02 |
| AC-04 | SCOPE AC-04; FR-05/07 | — |
| AC-05 | SCOPE AC-05; FR-06/07/10 | SR-02, SR-07 |
| AC-06 | SCOPE AC-06; FR-11 | SR-04 |
| AC-07 | SCOPE AC-07; FR-11 | SR-04 |
| AC-08 | FR-04 | A-1 |
| AC-09 | FR-08 | SR-05 |
| AC-10 | NFR-01 | SR-01 |
| AC-11 | FR-09 | D-4 |
| AC-12 | NFR-02 | — |

All seven SCOPE acceptance criteria (AC-01..AC-07) are carried forward; AC-08..AC-12 add
verification for constraints and assumptions the risk assessment flagged (A-1, SR-01, SR-05, D-4, NFR-02).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- capability #5478 (KI-CHAIN-XV, done_when satisfied by weak mode), #5475 (per-entry content_hash tamper-evidence), ADR-004 #74 (SHA-256 content hash — frozen format basis), lesson #3611 (multi-doc interface correction must not fix only the primary file — reinforces the two-site SR-06 requirement), pattern #4617 (hash-coverage vs emitted-rows gotcha). No new patterns to store — spec decisions are feature-specific (read-only tier).
