# Test Plan: Legacy DELETE Removal

**Component:** `crates/unimatrix-server/src/services/status.rs` and
`crates/unimatrix-server/src/mcp/tools.rs`
**Risks Covered:** R-01
**ACs Covered:** AC-01a, AC-01b

---

## Overview

Both legacy 60-day observation DELETE sites must be removed unconditionally. These
are structural correctness checks, not runtime behavior tests. They are verified by
grep assertions at Gate 3c. Running both GC policies concurrently is explicitly
prohibited (see IMPLEMENTATION-BRIEF constraint 7).

The two sites:
- **status.rs site** (AC-01a): lines ~1372–1384 in `status.rs` — the step 4
  "Observation retention cleanup" block.
- **tools.rs site** (AC-01b): lines ~1630–1642 in `tools.rs` — the FR-07 in-tool
  60-day DELETE.

---

## Verification Method

### AC-01a — status.rs site removed

**Assertion (grep):**
```bash
grep -r "DELETE FROM observations WHERE ts_millis" \
  crates/unimatrix-server/src/services/status.rs
```
Must return **no matches** (exit code 1 from grep, or zero lines of output).

This assertion is independent of AC-01b. Both must be checked separately.

---

### AC-01b — tools.rs site removed

**Assertion (grep):**
```bash
grep -r "DELETE FROM observations WHERE ts_millis" \
  crates/unimatrix-server/src/mcp/tools.rs
```
Must return **no matches** (exit code 1 from grep, or zero lines of output).

This assertion is independent of AC-01a. A single combined grep across both files
is NOT sufficient — it would pass if one site remains while the other is removed.

---

## Supporting Grep Assertions

The following assertions confirm no other forms of the legacy DELETE survive:

```bash
# No ts_millis-based observation deletes anywhere in the server crate
grep -r "ts_millis" crates/unimatrix-server/src/services/status.rs
grep -r "ts_millis" crates/unimatrix-server/src/mcp/tools.rs
```

Both must return no matches (or only matches that are unrelated to DELETE statements,
e.g. in comments).

---

## Additional Confirmation

After the removal, `status.rs` step 4 must contain the new cycle-based GC block
(calls to `list_purgeable_cycles`, `gc_cycle_activity`, `gc_unattributed_activity`).
This confirms the replacement happened, not a partial deletion.

```bash
grep "list_purgeable_cycles\|gc_cycle_activity\|gc_unattributed_activity" \
  crates/unimatrix-server/src/services/status.rs
```
Must return at least one match per method name.

---

## Historical Context

Entry #3579 (lesson-learned): previous waves have delivered new code correctly but
missed mandatory removal steps, allowing old and new policies to coexist. The Risk
Strategy explicitly requires two independent grep assertions for this reason — a
combined grep across both files would pass if one site is present while the other
is absent, and vice versa.

---

## Gate Blocking Status

Both AC-01a and AC-01b are **non-negotiable Gate 3c blockers**. Either legacy site
surviving in the delivered code constitutes a feature failure regardless of whether
all other tests pass.
