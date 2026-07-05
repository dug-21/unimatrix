# Risk-Based Test Strategy: nxs-014

> Cross-version hash chain wiring (weak mode) + transport-agnostic chain-verify core.
> Risks are scored against the SETTLED architecture (verify core in `unimatrix-store`,
> `validate_hashes` refactored to a thin caller, `Command::Verify` CLI, two-site correction
> write-path fix) and the concrete acceptance criteria AC-01..AC-12. Historical evidence:
> #4177 (tautological assertion caught at gate), #4473 (warn+continue masks failure-path),
> #5180 (green-on-skip false-green), #3611 (multi-site interface change fixed at one site only),
> #4617 (export hash covers emitted rows).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | **Two-site half-fix persists empty.** Fixing only the struct literal (`write_ext.rs:539-540`) compiles clean and the returned in-memory record looks correct, but the INSERT still binds `""`/`1` (`:582-583`) so the DB row is wrong. An in-memory-only test passes green over a broken DB. | High | High | **Critical** |
| R-02 | **Deprecated predecessor invisible to the core (architect Open Q1).** Predecessors are `Deprecated` (superseded). If `query_all_entries` (CLI) or the import loader filters to `Active`, every chained successor's `supersedes` target is absent from the map → `MissingPredecessor` fires on a perfectly clean corpus. False-positive integrity alarm on all real correction chains. | High | High | **Critical** |
| R-03 | **Legacy tolerance regression (empty `previous_hash` read as break).** New chain check does not skip `previous_hash == ""` the way `import/mod.rs:429` does → forward-only legacy corrected entries (D-2) fail verify as broken. Existing production data and existing import tests light up. | High | Med | **High** |
| R-04 | **`validate_hashes` refactor changes verdict on existing imports.** The core's link check keys on the authoritative `supersedes` edge (`successor.previous_hash == predecessor.content_hash`), strictly stronger than the old "`previous_hash` references *some* known hash" existence check. A prior-passing import fixture can newly fail; a prior-failing one can newly pass. Refactor is behavior-changing, not behavior-preserving, for the `content_hash` AND chain-link pair. | High | Med | **High** |
| R-05 | **Import no longer rejects a tampered corpus atomically.** Import maps a non-clean `ChainReport` to `Err` and must `ROLLBACK` before `COMMIT`. If the report→error mapping or the pre-COMMIT branch is wrong, a tampered export COMMITs — silent integrity loss at the exact boundary the feature exists to guard. | High | Low | **High** |
| R-06 | **Scope-creep into strong mode breaks frozen hash (AC-10).** An implementer folds `previous_hash` into `compute_content_hash`, breaking the `hash.rs` known-value vectors (`e3b0c442…`, `"Test: Content"`) and colliding every legacy `content_hash`. Non-Goal 1, but the temptation is inline. | High | Low | **Medium** |
| R-07 | **Export/import round-trip drops or coerces `previous_hash`/`version`.** `version` is `u32` in `EntryRecord`, bound as `i64`, serialized in export. A lossy round-trip (empty-string vs NULL, version reset) silently detaches the chain; import re-verify would then either pass on garbage or fail on clean data. | Med | Med | **Medium** |
| R-08 | **Empty predecessor `content_hash` at correction masked as legacy (A-1).** If `original.content_hash` is empty/stale, the successor inherits an empty `previous_hash`, which the core *tolerates as legacy* — degrading silently instead of failing. A real defect is laundered into an "unverifiable-legacy" skip. | Med | Med | **Medium** |
| R-09 | **`DanglingPreviousHash` / `MissingPredecessor` mis-classified.** A non-empty `previous_hash` with `supersedes == None`, or a predecessor id not in the map, must fail loud (not be silently skipped) yet must not fire on legitimate legacy (empty hash). Boundary between "skip" and "break" is easy to get wrong. | Med | Med | **Medium** |
| R-10 | **CLI verify contract violations (AC-09).** Wrong exit code on a break (green on tamper — cf. #5180), project-dir resolution error, or a read-write open where read-only is specified. Output must name the offending id; a summary-only "N problems" fails AC-04/AC-09. | Med | Med | **Medium** |
| R-11 | **README claim re-drifts (AC-06/07).** Under-correcting leaves "tamper-evident"; over-correcting under-sells shipped integrity. This is the exact drift #912 exists to fix, reproduced one level up if the claim boundary is not pinned to the ADR. | Med | Med | **Medium** |
| R-12 | **Fail-silent posture on a detected break (NFR-06).** A warn-and-continue or `is_clean()` that returns true on a populated `violations` vec returns green over a real inconsistency (cf. #4473 warn+continue, #4177 tautological pass). | Med | Low | **Low** |

## Risk-to-Scenario Mapping

### R-01: Two-site half-fix persists empty (Critical)
**Severity**: High **Likelihood**: High
**Impact**: The headline bug ships "fixed" while the DB column is still empty; the product's integrity promise remains unbacked and no test caught it. Evidence: #3611 (multi-site interface change fixed at one site only), #4177 (assertion that always passes).

**Test Scenarios**:
1. **DB read-back after correction (the false-green killer).** Drive a real `context_correct` through `correct_entry`. Then, on a **fresh query against the persisted row** (`SELECT previous_hash, version FROM entries WHERE id = <new_id>` via `entry_from_row` / a second read), assert `previous_hash == original.content_hash` and `version == original.version + 1`. This test **fails on a struct-only fix** (INSERT still binds `""`/`1`) and **passes only when the bind site is also fixed**.
2. **Negative control proving the test's teeth.** Assert that the same values read from the *returned in-memory `EntryRecord`* are NOT the sole assertion basis — a comment/marker test documents that an in-memory-only assertion passes on the half-fix (do not rely on it). The DB read-back is the authority.
3. Multi-hop (N=3) chain: read all rows back from DB; assert each hop's persisted `previous_hash == predecessor.content_hash` and versions are `1,2,3`.

**Coverage Requirement**: AC-01 and AC-02 are satisfied ONLY by assertions that read `previous_hash`/`version` back from the database after correction. Any in-memory-record assertion is insufficient and MUST NOT be the sole check (C-04).

### R-02: Deprecated predecessor invisible to the core (Critical)
**Severity**: High **Likelihood**: High
**Impact**: Every real correction chain false-alarms because the predecessor (Deprecated) is not in the entry set. The feature's own verify would reject clean data — blocking imports and lying to the operator. This is architect Open Q1, unresolved at spec time.

**Test Scenarios**:
1. **Deprecated-predecessor verify passes clean.** Build a chain where the predecessor is `Deprecated` (`superseded_by` set) and the successor is `Active` with a populated `previous_hash`. Run `verify_entries` over the corpus loaded by the actual production loader (`query_all_entries` for CLI; the import-transaction load for import). Assert `is_clean() == true` and **assert the Deprecated predecessor is present in the checked set** (e.g., `report.checked` counts it) — proving it was loaded, not just absent-and-ignored.
2. **Guard test on the loader itself.** Directly assert `query_all_entries()` returns rows of `status = Deprecated`. If it filters to Active, this test fails loud and localizes the defect to the loader, not the core.
3. Import-path variant: import an export whose predecessors are Deprecated; assert import chain-verify passes (they load via the in-flight transaction).

**Coverage Requirement**: Both verify callers (CLI `query_all_entries`, import loader) must load ALL statuses. A test must exercise a Deprecated predecessor on each path and assert it is counted as checked.

### R-03: Legacy tolerance regression (High)
**Severity**: High **Likelihood**: Med
**Impact**: Forward-only legacy entries (`previous_hash = ""`, D-2) fail verify → false integrity alarm on existing production data; existing clean import fixtures break.

**Test Scenarios**:
1. **Mixed corpus verifies clean.** Corpus with legacy entries (`previous_hash = ""`) AND new chained corrected entries. Assert `is_clean()`; assert `report.skipped_legacy` counts every empty-hash entry (not silently counted as checked-and-passed).
2. **Mutation on the mixed corpus fails loud, names the id.** Take the same mixed corpus, mutate a superseded entry's `content` in the DB, run verify, assert non-clean AND the violation names the offending `entry_id`. This is the paired positive/negative that proves the skip is scoped to legacy, not a blanket pass.
3. Genesis entry (`supersedes == None`, `previous_hash == ""`) is skipped as legacy, not flagged.

**Coverage Requirement**: A single corpus mixing legacy + chained + a mutation must verify clean before mutation and fail loud (naming id) after (AC-03, AC-05).

### R-04: `validate_hashes` refactor changes verdict (High)
**Severity**: High **Likelihood**: Med
**Impact**: The refactor to a thin caller over the shared core is behavior-changing. Existing import tests are the regression tripwire; if they are updated to match new output without scrutiny, a silent semantics change ships.

**Test Scenarios**:
1. **All pre-existing `validate_hashes` / import tests still pass** (or their diffs are justified by the strictly-stronger link check, not by loosened tolerance). Run the existing import test suite unchanged first.
2. Import now enforces **content-hash AND chain-link together** (single oracle, ADR-001): a corpus with a correct content-hash but a broken `previous_hash` link must fail import; a corpus with a correct link but mutated content must also fail import. Both halves proven on the import path, not just the CLI path.
3. Preserve the two prior behaviors: content-hash recompute (`:421`) and non-empty-`previous_hash` handling (`:429`) still hold.

**Coverage Requirement**: Existing import tests pass unmodified except where the diff is a documented consequence of the stronger link check; both AND-halves exercised via import.

### R-05: Import atomicity on a tampered corpus (High)
**Severity**: High **Likelihood**: Low
**Impact**: A tampered export COMMITs — the integrity guard fails open at its own boundary.

**Test Scenarios**:
1. Import a tampered export (broken chain link); assert import returns `Err`, the transaction ROLLBACKs, and **no rows from that import are present** in the DB afterward (query post-failure count == pre-import count).
2. Import a clean corrected export; assert COMMIT succeeds and rows are present with intact `previous_hash`/`version`.

**Coverage Requirement**: Post-failure DB state proves ROLLBACK (not just that `Err` was returned).

### R-06: Frozen-hash scope-creep (Medium priority; High severity)
**Severity**: High **Likelihood**: Low
**Impact**: Any change to `compute_content_hash` breaks known-value vectors and collides every legacy hash — the weak/strong boundary is crossed.

**Test Scenarios**:
1. `hash.rs` known-value vectors (`e3b0c442…` genesis, `"Test: Content"`) remain byte-identical and pass — the AC-10 tripwire.
2. `compute_content_hash` signature is `pub fn (title: &str, content: &str) -> String`, unchanged (compile-level assertion / grep tripwire in review).

**Coverage Requirement**: AC-10 — vectors unchanged, signature unchanged. A diff to either fails.

### R-07: Export/import round-trip lossy (Medium)
**Severity**: Med **Likelihood**: Med
**Impact**: Silent chain detachment through backup/restore.

**Test Scenarios**:
1. Multi-hop corrected chain **including a legacy empty-hash entry** → export → import; assert `previous_hash` and `version` are byte-identical after re-import (empty stays empty, populated stays populated, version not reset).
2. Assert import-time chain-verify passes on the clean re-import (AC-05).
3. Boundary: `version` at a large value survives the `u32`↔`i64` bind round-trip without truncation.

**Coverage Requirement**: AC-05 — values identical after round-trip AND import re-verify clean.

### R-08: Empty predecessor `content_hash` masked as legacy (Medium)
**Severity**: Med **Likelihood**: Med
**Impact**: A real bad-state (active entry with empty `content_hash`) is laundered into a legacy skip instead of failing.

**Test Scenarios**:
1. Attempt a correction whose `original` has an empty `content_hash`; assert the correction **fails with an error naming `original_id`** (AC-08, FR-04) rather than writing an empty `previous_hash`.
2. Confirm the failure occurs at correction time (write path), before any row is persisted.

**Coverage Requirement**: AC-08 — correction rejects an empty predecessor hash, naming the id.

### R-09: Dangling / MissingPredecessor mis-classification (Medium)
**Severity**: Med **Likelihood**: Med
**Test Scenarios**:
1. Non-empty `previous_hash` with `supersedes == None` → `DanglingPreviousHash` violation (fail loud, not skip).
2. `supersedes` points at an id absent from the corpus → `MissingPredecessor { predecessor_id }` (distinct from a link mismatch).
3. Correct link → no violation. Each `ViolationKind` variant has at least one scenario that produces exactly it.

**Coverage Requirement**: Each `ViolationKind` variant is produced by a dedicated scenario and named in the report.

### R-10: CLI verify contract (Medium)
**Severity**: Med **Likelihood**: Med
**Test Scenarios**:
1. Clean corpus → exit code `0` and a summary of entries/chains checked.
2. Tampered corpus → **non-zero** exit code and output naming the offending entry id (not just a count — cf. #5180 green-on-detect).
3. CLI opens the DB read-only (`open_readonly`) and resolves the project dir via `ensure_data_directory`; a missing/invalid project dir errors cleanly, does not panic.

**Coverage Requirement**: AC-09 — both exit-code branches and id-naming on the CLI surface.

### R-11: README claim re-drift (Medium)
**Severity**: Med **Likelihood**: Med
**Test Scenarios**:
1. Diff inspection: no unqualified "tamper-evident"/"tamper evident" claim about the correction chain vs a DB-write adversary remains; corrected text states tamper-**recorded** / correction-history integrity.
2. Threat model recorded durably in README AND/OR ADR-002 (AC-07) — not only in a transient artifact.
3. Shipped integrity (per-entry content_hash, append-only audit, authoritative supersession chain) is NOT under-sold.

**Coverage Requirement**: AC-06, AC-07 — claim boundary matches ADR-002; ships in the same PR (C-08).

### R-12: Fail-silent on a detected break (Low)
**Severity**: Med **Likelihood**: Low
**Test Scenarios**:
1. `is_clean()` returns `false` whenever `violations` is non-empty (property-style over each variant).
2. No verify surface warns-and-continues or returns green on a populated report (NFR-06). CLI exit and import `Err` both keyed to `!is_clean()`.

**Coverage Requirement**: NFR-06 — every non-clean report maps to a hard failure on every surface.

## Integration Risks

- **Loader ↔ core status coverage (R-02):** the core is pure and trusts its input slice; correctness depends entirely on each caller loading ALL statuses (Deprecated predecessors). The bug hides in the *caller*, not the core — test both callers, not just `verify_entries` in isolation.
- **Import transaction visibility (R-05):** import must feed the core entries from its **in-flight `BEGIN IMMEDIATE` connection** so it sees uncommitted rows; the CLI uses a read-only pool. A core that "works" under the CLI can still be wired wrong at import if it reads a committed snapshot instead of the transaction connection.
- **Single-oracle AND enforcement (R-04):** ADR-001's whole point is that no caller can run only half the check. A regression where import calls the core but *also* keeps a divergent legacy path (two oracles) reintroduces the drift the architecture removes.
- **Write path ↔ read path coupling (R-01):** the struct field and the INSERT bind are independent literals; the correction's return value and its persisted row can disagree. Tests must cross the persistence boundary.

## Edge Cases

- Genesis entry: `supersedes == None`, `previous_hash == ""` → legacy-skip, not `DanglingPreviousHash`.
- Single-entry corpus, empty corpus → `is_clean()`, `checked`/`skipped_legacy` counts sane, no panic.
- Legacy predecessor + new successor: successor's `previous_hash` populated but points at a legacy entry whose own `previous_hash` is empty — the *hop being verified* is the successor→predecessor link, which is checkable; the predecessor's own genesis link is legacy-skipped. Both hold in one chain.
- Very long chain (N large): O(n) single pass, no quadratic re-walk (NFR-05); versions `1..N` monotonic.
- `version` boundary near `u32`↔`i64` bind (R-07).
- Deprecated entry that is BOTH a predecessor and itself has a predecessor (mid-chain): loaded and checked on both hops.

## Security Risks

Untrusted input surfaces this feature adds/touches:

- **Import file (untrusted export):** attacker-supplied `previous_hash`, `version`, `content`, `supersedes`. Blast radius is bounded by R-05 atomicity — a tampered corpus must ROLLBACK, never COMMIT. Content-hash recompute + chain-link check are the gate. **Out of tier (Non-Goal 3):** a DB-write adversary who recomputes both the content hash and the successor link defeats detection — this is the stated, documented limitation, not a test gap. Do not write a test asserting detection of a perfectly-coordinated multi-row rewrite.
- **CLI `project_dir` argument:** resolves to a DB path via `ensure_data_directory`. Path is operator-supplied and local; open is **read-only** so a mis-pointed path cannot corrupt data. Risk is low (local operator, single-tenant personal cloud); assert read-only open so a future change cannot silently widen it to read-write.
- **`ChainReport` / error output:** names entry ids and hash values (not raw content). Confirm the report does not echo full untrusted `content` into logs/CLI output in a way that enables terminal-escape injection; ids + hashes are safe.
- **Resource use:** verify is O(entries) with an in-memory `HashMap`; a very large import is bounded by existing import limits, not newly introduced here. No new unbounded recursion (chain walk is a map lookup, not recursion).

## Failure Modes

- **Detected break (any surface):** fail loud, name the offending `entry_id` and `ViolationKind`. Never warn-and-continue, never green (NFR-06, R-12).
- **Import, tampered corpus:** ROLLBACK before COMMIT; DB unchanged; `Err` surfaced (R-05).
- **CLI, tampered corpus:** non-zero exit, human-readable break report (R-10).
- **Correction, empty predecessor `content_hash`:** reject the correction naming `original_id`; persist nothing (R-08, AC-08).
- **Correction, normal:** both struct and bind sites persist the link; returned record and DB row agree (R-01).
- **Legacy / genesis entry:** treated as unverifiable-legacy skip, corpus stays clean (R-03).

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (scope-creep into strong mode / frozen hash) | R-06 | ADR-002 pins the frozen-hash boundary; AC-10 tripwire on `hash.rs` vectors + signature. |
| SR-02 (legacy empty-hash treated as break) | R-03 (+ R-09 boundary) | Core skips empty `previous_hash` as legacy (ADR-003); mixed-corpus clean + mutation-fails-loud test (AC-03/05). |
| SR-03 (verify-core crate placement) | R-04, R-05, integration (loader↔core) | ADR-001 places `verify_entries` in `unimatrix-store` (leaf crate); `validate_hashes` + CLI are thin callers, one oracle. Tested via both callers. |
| SR-04 (README claim boundary subjective) | R-11 | ADR-002 fixes the exact claim boundary; AC-06/07 diff + durability check. |
| SR-05 (new CLI verify surface under-specified) | R-10 | FR-08 CLI contract; AC-09 both exit-code branches + id-naming. |
| SR-06 (two-literal half-fix) | R-01 | Both sites in ADR-003; AC-01/02 **DB read-back** tests fail on a struct-only fix (C-04). |
| SR-07 (export/import round-trip) | R-07 | AC-05 multi-hop + legacy round-trip, values identical + import re-verify clean. |
| — (architect Open Q1: `query_all_entries` status coverage) | R-02 | Not a scope SR — surfaced by the architecture. Loader must return Deprecated; tested on both callers with a Deprecated predecessor counted as checked. |
| A-1 (predecessor `content_hash` populated) | R-08 | FR-04 correction rejects empty predecessor hash (AC-08). |
| A-2 (supersedes chain trusted) | R-09 (bounded) | Non-Goal 6; core verifies hashes along the trusted edge, not topology. Dangling/missing predecessor still fail loud. |
| A-3 (empty-hash convention load-bearing) | R-03 | Core matches `import/mod.rs:429` convention; regression tests guard it. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 6 — DB read-back half-fix killer, multi-hop DB read-back, Deprecated-predecessor clean-verify (both callers), loader-returns-Deprecated guard |
| High | 3 (R-03, R-04, R-05) | 8 — mixed-corpus clean + mutation-fails-loud, existing import tests unchanged, AND-halves via import, tampered-import ROLLBACK + state check, clean-import COMMIT |
| Medium | 6 (R-06, R-07, R-08, R-09, R-10, R-11) | 14 — frozen-vector tripwire, round-trip incl. legacy, empty-predecessor reject, each ViolationKind, CLI both exit branches + read-only open, README diff + ADR durability |
| Low | 1 (R-12) | 2 — is_clean property, no-fail-silent on every surface |

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search for false-green / gate-rejection lessons -- #4177 (tautological assertion caught at gate), #4473 (warn+continue masks failure-path tests), #5180 (green-on-skip false-green in CI), #3611 (multi-site interface change fixed at one site only — reinforces R-01 two-site), #4617 (export hash covers emitted rows — informs R-07). Searched hash-chain/SQLite-migration risk patterns -- no nxs-014-specific verify-chain pattern exists yet; migration patterns (#374/#681/#4092) not applicable (weak mode = no migration).
- Stored: nothing novel to store -- the risks here are feature-specific (two-site half-fix, Deprecated-predecessor loader coverage) and already captured by existing lessons #3611/#4177/#4473. No cross-feature pattern visible across 2+ features beyond what is already stored; storing would duplicate #3611 and #4177.
