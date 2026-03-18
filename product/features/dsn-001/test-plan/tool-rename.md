# dsn-001 Test Plan — tool-rename

Components:
- `crates/unimatrix-server/src/mcp/tools.rs` (primary)
- `crates/unimatrix-observe/src/session_metrics.rs`
- `product/test/infra-001/harness/client.py`
- `product/test/infra-001/suites/test_protocol.py`
- `product/test/infra-001/suites/test_tools.py`
- All non-Rust files per SR-05 checklist

Risks covered: R-04, AC-13, AC-14.

---

## Scope of Changes

`context_retrospective` → `context_cycle_review` across all files.
This is a hardcoded rename, not runtime config. Build passing is insufficient —
all non-Rust files must be verified.

---

## Mandatory Static Gate (R-04, AC-13)

This grep sweep must be run as a pre-PR gate and its output recorded in
RISK-COVERAGE-REPORT.md.

```bash
grep -r "context_retrospective" . \
    --include="*.rs" --include="*.py" --include="*.md" --include="*.toml" \
    --exclude-dir=.git \
    --exclude-dir=product/features/col-002 \
    --exclude-dir=product/features/col-002b \
    --exclude-dir=product/features/col-009 \
    --exclude-dir=product/features/col-010 \
    --exclude-dir=product/features/col-010b \
    --exclude-dir=product/features/col-012 \
    --exclude-dir=product/features/col-014 \
    --exclude-dir=product/features/col-016 \
    --exclude-dir=product/features/col-017 \
    --exclude-dir=product/features/col-020 \
    --exclude-dir=product/features/col-020b \
    --exclude-dir=product/features/col-022 \
    --exclude-dir=product/features/vnc-005 \
    --exclude-dir=product/features/vnc-008 \
    --exclude-dir=product/features/vnc-009 \
    --exclude-dir=product/features/vnc-011 \
    --exclude-dir=product/features/nxs-008 \
    --exclude-dir=product/features/nxs-009 \
    --exclude-dir=product/features/crt-011 \
    --exclude-dir=product/features/crt-018 \
    --exclude-dir=product/features/crt-018b \
    --exclude-dir=product/features/bugfix-236 \
    --exclude-dir=product/research/ass-007 \
    --exclude-dir=product/research/ass-014 \
    --exclude-dir=product/research/ass-015 \
    --exclude-dir=product/research/ass-016 \
    --exclude-dir=product/research/ass-018 \
    --exclude-dir=product/research/ass-020 \
    --exclude-dir=product/research/ass-022 \
    --exclude-dir=product/research/optimizations
```

Expected result: **zero matches** outside the excluded historical directories.

Any match outside those directories is a missed update that must be fixed before merge.

---

## Unit Test: `classify_tool` Updated (R-04 scenario 4)

In `crates/unimatrix-observe/src/session_metrics.rs`, the test assertion at line 601
must be updated:

```rust
// Before dsn-001:
assert_eq!(classify_tool("context_retrospective"), "other");

// After dsn-001:
assert_eq!(classify_tool("context_cycle_review"), "other");
```

The old test assertion must not exist. Run:
```bash
grep "context_retrospective" crates/unimatrix-observe/src/session_metrics.rs
```
Must return zero results after the update.

The new test:
```rust
fn test_classify_tool_context_cycle_review() {
    assert_eq!(classify_tool("context_cycle_review"), "other",
        "context_cycle_review must be classified as 'other'");
}
```

---

## Integration Test: Tool List (AC-13 positive + negative)

In `product/test/infra-001/suites/test_protocol.py`, the existing tool list test
at line 55 must be updated to assert:

```python
def test_tool_discovery_includes_cycle_review(server):
    """AC-13: context_cycle_review is in the tool list."""
    resp = server.list_tools()
    tool_names = [t["name"] for t in resp["tools"]]
    assert "context_cycle_review" in tool_names, \
        f"context_cycle_review must be in tool list; got: {tool_names}"
    assert "context_retrospective" not in tool_names, \
        f"context_retrospective must not be in tool list; got: {tool_names}"
```

This test is in the `protocol` suite and must be run in Stage 3c.

---

## Integration Test: Renamed Tool Responds (AC-13 live call)

```python
def test_cycle_review_renamed_tool_responds(server):
    """AC-13: context_cycle_review is callable and returns structured output."""
    resp = server.context_cycle_review(feature_cycle="col-022")
    # The tool must not return a "tool not found" error.
    assert resp is not None
    assert "error" not in resp or resp.get("status") != "tool_not_found"
    # Structural response validation — same format as pre-rename.
    # Accept any successful response: ok status, or report key present.
    assert resp.get("status") == "ok" or "report" in resp or "analysis" in resp, \
        f"context_cycle_review must return structured output; got: {resp}"
```

Fixture: `server` (fresh DB). The `feature_cycle = "col-022"` is an existing
completed cycle with known data; response format unchanged by the rename.

---

## Non-Rust File Update Completeness Checklist

The following file updates are required by SPECIFICATION.md §SR-05. Each must be
verified in Stage 3c by inspection or targeted grep:

| File | Required Change | Verification |
|------|----------------|--------------|
| `crates/unimatrix-server/src/mcp/tools.rs` | `#[tool(name)]`, handler fn, 2 audit strings, 3+ doc strings | `grep context_retrospective` → 0 |
| `crates/unimatrix-server/src/server.rs` | 3 doc comments | `grep context_retrospective` → 0 |
| `crates/unimatrix-observe/src/types.rs` | 1 doc comment | `grep context_retrospective` → 0 |
| `crates/unimatrix-observe/src/session_metrics.rs` | 1 test assertion | `grep context_retrospective` → 0 |
| `product/test/infra-001/harness/client.py` | method rename + tool name string | `grep context_retrospective` → 0 |
| `product/test/infra-001/suites/test_protocol.py` | tool name in expected list | `grep context_retrospective` → 0 |
| `product/test/infra-001/suites/test_tools.py` | ~14 call sites + section headers | `grep context_retrospective` → 0 |
| `.claude/skills/uni-retro/SKILL.md` | `mcp__unimatrix__context_cycle_review` | `grep context_retrospective` → 0 |
| `.claude/protocols/uni/uni-agent-routing.md` | 1 reference | `grep context_retrospective` → 0 |
| `packages/unimatrix/skills/retro/SKILL.md` | `context_cycle_review` | `grep context_retrospective` → 0 |
| `product/workflow/base-001/protocol-evolved/uni-agent-routing.md` | 1 reference | `grep context_retrospective` → 0 |
| `product/PRODUCT-VISION.md` | lines 32, 43, 282, 819 | `grep context_retrospective` → 0 |
| `README.md` | tool table row | `grep context_retrospective` → 0 |
| `product/ALPHA_UNIMATRIX_COMPLETED_VISION.md` | references | `grep context_retrospective` → 0 |

All checks pass when the single repo-wide grep sweep returns zero matches outside
excluded historical directories.

---

## `CycleParams.topic` Doc Neutralization (AC-14)

**Verification method**: manual code review (not a test).

Read `CycleParams.topic` doc comment in `crates/unimatrix-server/src/mcp/tools.rs`.
Assert:
- The word "feature" does not appear as the primary or sole example.
- The comment references multiple domain examples: feature, incident, campaign,
  case, sprint, or experiment (per SPECIFICATION.md AC-14).
- The doc communicates a "bounded unit of work tracked by any domain."

This is a manual read-and-assess verification. In Stage 3c, include in the
RISK-COVERAGE-REPORT.md under AC-14: "Verified: CycleParams.topic doc updated —
references [examples found]."

---

## Audit Log Strings in `tools.rs` (R-04 scenario 5)

The two audit log strings in `tools.rs` must reference the new name:

```bash
grep "context_retrospective" crates/unimatrix-server/src/mcp/tools.rs
```

Expected zero results. Specifically, lines formerly containing:
- `operation: "context_retrospective".to_string()`
- `operation: "context_retrospective/lesson-learned".to_string()`

Must now be:
- `operation: "context_cycle_review".to_string()`
- `operation: "context_cycle_review/lesson-learned".to_string()`
