# Security Review: nxs-014-security-reviewer

## Risk Level: low

## Summary
Fresh-context security review of PR #914 (cross-version hash-chain weak mode + chain-verify core).
No injection, no new dependencies, no secrets, and the integrity claim is honestly scoped to
tamper-RECORDED. The flagged regression (stronger supersedes-keyed link check) is safe on real
data because `previous_hash` was universally empty before this fix. No blocking findings.

## Findings

### F-1 — SQL injection surface (validate_hashes / verify loaders): clean
- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/import/mod.rs:401`, `crates/unimatrix-store/src/read.rs:324`
- **Description**: `validate_hashes` builds SQL via `format!("SELECT {} FROM entries ORDER BY id", ENTRY_COLUMNS)`. `ENTRY_COLUMNS` is a compile-time `pub const &str`, not user input. `verify.rs` issues no raw SQL. Every value bind across the write/verify paths uses positional parameters. No untrusted string reaches a query.
- **Recommendation**: None. Keep `ENTRY_COLUMNS` a constant; never interpolate row data.
- **Blocking**: no

### F-2 — Regression from the stronger supersedes-keyed link check: bounded, fails safe
- **Severity**: low
- **Location**: `crates/unimatrix-store/src/chain_verify.rs:166-203`
- **Description**: The refactor adds `DanglingPreviousHash` (populated `previous_hash` + `supersedes == None`) and `MissingPredecessor`, and keys `ChainLinkMismatch` on the authoritative `supersedes` edge — strictly stronger than the old "references *some* known hash" existence check. The concern is legitimate older exports failing verify. Verified against the codebase: historically `correct_entry` hardcoded `previous_hash = ""` / `version = 1` (the exact GH #912 defect), and no other production writer populates `previous_hash` to a non-empty value (all other paths bind `''`; only `correct_entry` populates it, always co-set with `supersedes`). Therefore no real pre-fix export can carry a populated `previous_hash` with `supersedes == None`, so `DanglingPreviousHash` cannot false-fire on legitimate data — legacy rows carry empty `previous_hash` and legacy-skip. The failure direction if this reasoning were ever violated is a loud rejection (false positive), never silent corruption.
- **Recommendation**: None required. The loader-returns-all-statuses guard test already covers R-02.
- **Blocking**: no

### F-3 — False-clean (verify misses a real break): only the documented out-of-tier case
- **Severity**: low
- **Location**: `crates/unimatrix-store/src/chain_verify.rs:140-207`
- **Description**: The core runs the content-hash recompute AND the chain-link check together (no early `continue` between them — an entry can record both violations). The only undetected tamper is a DB-write adversary who recomputes both the mutated entry's `content_hash` and the single successor's `previous_hash` (O(1), no cascade), or who sets `previous_hash=""` to masquerade a broken link as legacy. Both are explicitly out-of-tier per ADR-002 (that adversary owns the bearer token and all secrets). This is a documented limitation, not a defect.
- **Recommendation**: None. Do not add a test asserting detection of a coordinated multi-row rewrite (would contradict the threat model).
- **Blocking**: no

### F-4 — Correcting a legacy Active entry with empty content_hash now hard-errors
- **Severity**: low (informational — behavioral change)
- **Location**: `crates/unimatrix-store/src/write_ext.rs:489-497`
- **Description**: A correction whose predecessor has an empty `content_hash` is now rejected (AC-08), where previously it silently produced an empty `previous_hash`. Real Active entries receive a `content_hash` at insert, so this only fires on genuinely corrupt state. The check runs after the read and before any mutation (verified: ID allocation, Deprecate UPDATE and INSERT all follow), so nothing is persisted on rejection. Operator-visible change: a corrupt legacy entry must have its hash repaired before it can be corrected.
- **Recommendation**: None; intended and safe. Noted for release/operator awareness.
- **Blocking**: no

### F-5 — Report output does not echo untrusted content: clean
- **Severity**: low (informational)
- **Location**: `crates/unimatrix-store/src/chain_verify.rs:43-61,107-133`
- **Description**: `ChainReport::describe()` and `ViolationKind::Display` emit only entry ids and hash values, never raw `title`/`content`. No terminal-escape / log-injection surface from attacker-controlled entry content.
- **Blocking**: no

### F-6 — Integrity claim honesty (README vs ADR-002): correct
- **Severity**: low (informational)
- **Location**: `README.md:235,724`
- **Description**: Both README integrity sections were changed from "tamper-evident … any break is detectable" to "tamper-**recorded**", with an explicit out-of-tier statement about the raw-DB-write adversary matching ADR-002. No residual unqualified "tamper-evident" claim about the correction chain remains. Shipped integrity (per-entry content_hash, `previous_hash` chain, `unimatrix verify`) is stated without under-selling.
- **Blocking**: no

## Blast Radius Assessment
Worst case if the write path has a subtle bug: `previous_hash`/`version` are corrupted at correction
time. Mitigated — both the struct literal (`write_ext.rs:555`) and the INSERT bind (`:601`) now derive
from the same record, and tests assert values read **back from the DB** (not the in-memory record), so a
struct-only half-fix fails. Worst case if verify has a subtle bug: a tampered import COMMITs. Mitigated —
a non-clean `ChainReport` maps to `Err` on the existing ROLLBACK-before-COMMIT path; a test proves the
post-failure row count is 0. The dangerous direction (silent false-clean) is confined to the documented
out-of-tier adversary; all in-tier failures surface loud (Err/non-zero exit, offending id named).

## Regression Risk
Low. The behavior-changing refactor (supersedes-keyed link check) cannot false-fire on real pre-fix
data because `previous_hash` was universally empty before this feature. The one new hard-error path
(empty predecessor content_hash) only triggers on genuine corruption. Verify opens the DB read-only, so
a mis-pointed `--project-dir` cannot mutate data. No schema migration (version stays 30); `previous_hash`
is `NOT NULL DEFAULT ''`, so no NULL-vs-empty coercion hazard on round-trip.

## Dependency Safety
No new dependencies. The import refactor removed `std::collections::HashSet` and `sqlx::Row` imports.
The verify core reuses existing store internals. No `cargo audit` surface change.

## Secrets
None. No hardcoded credentials, tokens, or keys in the diff.

## PR Comments
- Posted 1 review comment on PR #914 (--comment, non-blocking).
- Blocking findings: no.

## Knowledge Stewardship
- Stored: nothing novel to store -- all findings are feature-specific and confirm the design already
  encoded the risks (R-01..R-12). The generalizable anti-patterns here (read-back-from-DB not in-memory;
  single verify oracle; content-not-echoed-in-reports) are already captured by existing lessons
  (#3611 multi-site half-fix, vnc-034 single-oracle). Storing would duplicate.
