# Security Review: bugfix-369-security-reviewer

## Risk Level: low

## Summary

Commit `0f646cc` is a pure removal of the dead-knowledge auto-deprecation pass
(`dead_knowledge_deprecation_pass`, `fetch_recent_observations_for_dead_knowledge`,
`DEAD_KNOWLEDGE_SESSION_THRESHOLD`, `DEAD_KNOWLEDGE_DEPRECATION_CAP`) and all
associated tests from `crates/unimatrix-server/src/background.rs`. No new code was
introduced. The detection helpers in `unimatrix-observe` are retained untouched. The
one-shot `run_dead_knowledge_migration_v1` is retained and continues to compile and
pass its tests. No security concerns were identified.

---

## Findings

### Finding 1: Stale doc comments in unimatrix-observe reference the removed function

- **Severity**: low
- **Location**:
  - `crates/unimatrix-observe/src/extraction/dead_knowledge.rs` line 9
  - `crates/unimatrix-observe/src/extraction/mod.rs` line 198
- **Description**: Two doc comments still refer to
  `background::dead_knowledge_deprecation_pass()` as the consumer of the detection
  helpers. That function no longer exists. The references are in doc/comment text
  only — no functional code, no dead symbol, no broken call. The compiler emits no
  error or warning for these.
- **Recommendation**: Update the two comments to note that the deprecation pass was
  removed in GH #369 and the helpers are retained for future curation tooling
  (GH #370). Non-urgent.
- **Blocking**: no

### Finding 2: No orphan import or symbol — confirmed

- **Severity**: info
- **Location**: `crates/unimatrix-server/src/background.rs` line 20 (import block)
- **Description**: The removed import
  `use unimatrix_observe::extraction::dead_knowledge::detect_dead_knowledge_candidates`
  was correctly removed in the diff. `cargo check` produced zero errors; the remaining
  import block is clean.
- **Recommendation**: None.
- **Blocking**: no

### Finding 3: No new input validation surface introduced

- **Severity**: info
- **Description**: The change removes code; it does not add any new external input
  paths, deserialization, SQL queries, file operations, or shell invocations. OWASP
  injection, path traversal, and broken access-control checks are not applicable to a
  pure removal.
- **Recommendation**: None.
- **Blocking**: no

### Finding 4: No hardcoded secrets or credentials

- **Severity**: info
- **Description**: Full diff reviewed. No API keys, tokens, passwords, or credentials
  were introduced or removed.
- **Recommendation**: None.
- **Blocking**: no

---

## Blast Radius Assessment

The removed code ran inside the background maintenance tick, which is a non-user-facing
fire-and-forget path. The worst case if the removal has a subtle unintended side effect
is that the tick no longer performs automatic deprecation of stale knowledge entries.
This is the intended outcome — the function was causing over-deprecation (#367 caused
~495 valid entries to be incorrectly deprecated). The failure mode is additive omission
(entries remain Active longer than they should), not data corruption or privilege
escalation. The retained `run_dead_knowledge_migration_v1` one-shot migration is
COUNTERS-gated and idempotent; it is unaffected by this removal.

No other callers of the removed symbols exist in the codebase. Grep across the full
workspace confirmed all four removed symbols
(`dead_knowledge_deprecation_pass`, `fetch_recent_observations_for_dead_knowledge`,
`DEAD_KNOWLEDGE_SESSION_THRESHOLD`, `DEAD_KNOWLEDGE_DEPRECATION_CAP`) appear only in
historical product doc files (bugfix-367 and bugfix-351 reports) and in
`dead_knowledge.rs` doc comments — not in any live Rust source.

---

## Regression Risk

**Low.** The only live behavioral change is that the maintenance tick no longer fires
`dead_knowledge_deprecation_pass` on every 15-minute cycle. Existing functionality
(search, store, confidence scoring, coherence gate, auto-quarantine, the one-shot
migration) is entirely unaffected.

The two tests that exercised the removed function
(`test_dead_knowledge_deprecation_pass_unit`,
`test_dead_knowledge_pass_session_threshold_boundary`) were also removed. The
`migration_v1` tests remain and pass:

```
test background::tests::test_dead_knowledge_migration_v1_is_idempotent ... ok
test background::tests::test_dead_knowledge_migration_v1_deprecates_legacy_entries ... ok
```

`cargo check -p unimatrix-server` completed with zero errors (10 pre-existing dead-code
and unused-import warnings, all pre-existing, none introduced by this commit).

---

## PR Comments

This commit was merged directly to `main` without a PR. No PR comments were posted
(no PR exists to comment on).

---

## Knowledge Stewardship

Nothing novel to store — the pattern here (removing an unsafe recurring mutation pass
from a background tick after it caused mass data corruption) is already covered by the
existing lesson-learned entry from bugfix-367. The generalizable anti-pattern
("irreversible status mutations in background ticks need strong signal confidence before
activation") is captured in that prior entry.

---

## Self-Check

- [x] Full git diff was read (`git show 0f646cc`)
- [x] Root cause analysis context read (bugfix-367 and bugfix-351 reports via grep
      output; commit message is self-documenting)
- [x] Affected source file `background.rs` read at relevant sections (import block,
      tick function, retained migration code)
- [x] OWASP concerns evaluated — not applicable to a pure code removal
- [x] Blast radius assessed — worst case is additive omission (valid entries stay
      Active), not data loss
- [x] Input validation checked — no new inputs introduced
- [x] No hardcoded secrets in the diff
- [x] PR comments — N/A (direct merge to main, no PR)
- [x] Risk level accurately reflects findings (low)
- [x] Report written to correct path
- [x] Knowledge Stewardship block included
