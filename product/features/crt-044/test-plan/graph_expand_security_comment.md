# Test Plan: graph_expand_security_comment

## Component

`crates/unimatrix-engine/src/graph_expand.rs` — `// SECURITY:` comment added immediately
before `pub fn graph_expand(` signature.

## Verification Method

**Static grep check only. No runtime test.**

This is documented and accepted per ADR-003 (entry #4081). The change is documentation-only
(C-07): zero logic change, zero behavioral change, no new code paths. A runtime test cannot
verify comment text accuracy — it can only verify the comment's presence in the file.

---

## Verification: AC-08

**Command (Stage 3c)**:

```bash
grep -n '// SECURITY:' crates/unimatrix-engine/src/graph_expand.rs
```

**Expected output**: At least one line containing `// SECURITY:` at or immediately before the
`pub fn graph_expand(` line.

**Pass condition**: Command exits 0 and output is non-empty.

**Fail condition**: Command exits 0 but output is empty (comment not present), OR command exits
non-zero (file not found — wrong path).

**Additional verification**:

```bash
grep -n 'pub fn graph_expand' crates/unimatrix-engine/src/graph_expand.rs
```

Cross-reference the line numbers: the `// SECURITY:` comment must appear on the line(s)
immediately preceding the `pub fn graph_expand(` line.

**Required comment text** (FR-S-01):
```
// SECURITY: caller MUST apply SecurityGateway::is_quarantined() before inserting
// returned IDs into result sets. graph_expand performs NO quarantine filtering.
```

---

## What Is NOT Tested (and Why)

| Aspect | Why not tested |
|--------|----------------|
| Comment text accuracy over time | Accepted per ADR-003. The module-level doc block (lines 12-18) and the `search.rs` call site are authoritative; the comment is a visibility aid only. No test can verify comment correctness — only presence. |
| `graph_expand` logic unchanged | Covered by existing `graph_expand` unit tests. If the comment accidentally modifies logic (impossible for a `//` comment), those tests catch it. No new test needed here. |
| Quarantine obligation enforced at call sites | Covered by existing `search.rs` tests / security test suite. Out of scope for this component. |

---

## Risk Coverage

| Risk | Coverage |
|------|----------|
| R-08 (comment becomes stale) | Accepted per ADR-003. Static grep verifies presence, not accuracy. Future refactors of `SecurityGateway` should include a `grep '// SECURITY:'` scan as a pre-merge checklist item. |

---

## Stage 3c Execution Note

The grep checks for AC-08 must be run from the repository root:

```bash
# From /workspaces/unimatrix
grep -c '// SECURITY:' crates/unimatrix-engine/src/graph_expand.rs
# Expected: output is "2" (two comment lines)

grep -n 'pub fn graph_expand' crates/unimatrix-engine/src/graph_expand.rs
# Note the line number N; then verify // SECURITY: appears at N-2 and N-1.
```

Result is recorded in RISK-COVERAGE-REPORT.md under AC-08 with the grep output as evidence.

---

*Authored by crt-044-agent-2-testplan (claude-sonnet-4-6). Written 2026-04-03.*
