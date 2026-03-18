# Pseudocode: tool-rename

**Primary file**: `crates/unimatrix-server/src/mcp/tools.rs` (modified)
**Blast radius**: 31 locations across 14 files (see SR-05 checklist in SPECIFICATION.md)

## Purpose

Renames the MCP tool `context_retrospective` to `context_cycle_review` throughout
the entire repository. Neutralizes the `CycleParams.topic` field doc to remove
Agile/SDLC vocabulary. This is a non-configurable hardcoded rename — it affects
tool routing, audit log strings, doc comments, protocol files, skill files, Python
tests, and product documents.

Build passing is explicitly insufficient. The SR-05 checklist requires all 31
locations to be updated.

---

## `mcp/tools.rs` Changes

### `#[tool(name)]` Attribute

```
// BEFORE:
#[tool(
    name = "context_retrospective",
    description = "Analyze observation data for a feature cycle. ..."
)]
async fn context_retrospective(

// AFTER:
#[tool(
    name = "context_cycle_review",
    description = "Analyze observation data for a work cycle. ..."
)]
async fn context_cycle_review(
```

The description should be updated to replace "feature cycle" with "work cycle"
or equivalent domain-neutral language for alignment with FR-12.

### Handler Function Rename

```
// BEFORE:
async fn context_retrospective(
    &self,
    Parameters(params): Parameters<RetrospectiveParams>,
) -> Result<CallToolResult, McpError>

// AFTER:
async fn context_cycle_review(
    &self,
    Parameters(params): Parameters<RetrospectiveParams>,
) -> Result<CallToolResult, McpError>
```

Note: `RetrospectiveParams` struct name may remain as-is (it is an internal
implementation detail, not a public tool interface). Or it can be renamed to
`CycleReviewParams` for clarity. The delivery agent should choose consistency
within the file. The `#[tool(name)]` attribute governs the public name.

### Audit Log Strings (2 locations)

```
// BEFORE (line ~1457):
operation: "context_retrospective".to_string(),

// AFTER:
operation: "context_cycle_review".to_string(),

// BEFORE (line ~1734):
operation: "context_retrospective/lesson-learned".to_string(),

// AFTER:
operation: "context_cycle_review/lesson-learned".to_string(),
```

### Doc Strings in `tools.rs`

```
// BEFORE (line ~239, struct doc):
/// Parameters for the context_retrospective tool.

// AFTER:
/// Parameters for the context_cycle_review tool.

// BEFORE (line ~1617, helper doc):
/// Called inside a tokio::spawn from context_retrospective.

// AFTER:
/// Called inside a tokio::spawn from context_cycle_review.

// BEFORE (lines ~1505, ~1560 — context_cycle tool doc references):
/// Attribution is best-effort via the hook path; confirm via context_retrospective.
/// ...
/// Use context_retrospective to confirm session attribution.

// AFTER (both):
/// Attribution is best-effort via the hook path; confirm via context_cycle_review.
/// ...
/// Use context_cycle_review to confirm session attribution.
```

### `CycleParams.topic` Field Doc Neutralization (FR-12)

```
// The CycleParams struct is used by context_cycle (not context_cycle_review).
// Its topic field doc currently references "feature" as the canonical example.

// BEFORE (approximate):
/// The feature cycle identifier (e.g., "col-022") for the work being tracked.

// AFTER (domain-neutral):
/// The cycle identifier for the bounded unit of work being tracked.
/// Examples: a feature ("col-022"), incident ("inc-045"), campaign, case, sprint, or experiment.
/// The format is domain-defined; Unimatrix treats it as an opaque string identifier.
```

The word "feature" must NOT be the primary or only example. The doc must convey
domain-agnostic meaning (FR-12, AC-14).

---

## `server.rs` Doc Comment Updates

Three doc comments in `server.rs` reference `context_retrospective` (also listed in
server-instructions.md for coordination):

```
// Line ~65:
// BEFORE: /// context_retrospective handler (drains on call).
// AFTER:  /// context_cycle_review handler (drains on call).

// Line ~147:
// BEFORE: /// features that complete without calling context_retrospective or context_cycle.
// AFTER:  /// features that complete without calling context_cycle_review or context_cycle.

// Line ~207:
// BEFORE: /// Shared with UDS listener; drained by context_retrospective handler.
// AFTER:  /// Shared with UDS listener; drained by context_cycle_review handler.
```

---

## `unimatrix-observe` Crate Updates

### `crates/unimatrix-observe/src/types.rs`

```
// Line ~221:
// BEFORE: /// Complete analysis output returned by context_retrospective.
// AFTER:  /// Complete analysis output returned by context_cycle_review.
```

### `crates/unimatrix-observe/src/session_metrics.rs`

```
// Line ~601:
// BEFORE:
assert_eq!(classify_tool("context_retrospective"), "other");

// AFTER:
assert_eq!(classify_tool("context_cycle_review"), "other");
```

---

## Python Integration Test Files

### `product/test/infra-001/harness/client.py`

```
# Line ~629:
# BEFORE:
def context_retrospective(self, ...):
    return self.call_tool("context_retrospective", args, ...)

# AFTER:
def context_cycle_review(self, ...):
    return self.call_tool("context_cycle_review", args, ...)
```

### `product/test/infra-001/suites/test_protocol.py`

```
# Line ~55:
# BEFORE: "context_retrospective" in tool list
# AFTER:  "context_cycle_review" in tool list
```

### `product/test/infra-001/suites/test_tools.py`

All 11 occurrences (section headers + call sites):
```
# Section headers:
# BEFORE: # === context_retrospective (col-002) ===
# AFTER:  # === context_cycle_review (col-002) ===

# BEFORE: # === context_retrospective baseline comparison (col-002b) ===
# AFTER:  # === context_cycle_review baseline comparison (col-002b) ===

# BEFORE: # === context_retrospective format dispatch (vnc-011) ===
# AFTER:  # === context_cycle_review format dispatch (vnc-011) ===

# All resp = server.context_retrospective(...) calls:
# BEFORE: resp = server.context_retrospective(...)
# AFTER:  resp = server.context_cycle_review(...)
```

Lines affected: ~773, ~779, ~785, ~893, ~897, ~935, ~939, ~966, ~996, ~1009, ~1022
plus doc string on line ~814.

---

## Protocol and Skill Files

### `.claude/skills/uni-retro/SKILL.md`

```
# Line ~29:
# BEFORE: mcp__unimatrix__context_retrospective(feature_cycle: ...)
# AFTER:  mcp__unimatrix__context_cycle_review(feature_cycle: ...)
```

### `.claude/protocols/uni/uni-agent-routing.md`

```
# Line ~151:
# BEFORE: Data gathering (context_retrospective + artifact review)
# AFTER:  Data gathering (context_cycle_review + artifact review)
```

### `packages/unimatrix/skills/retro/SKILL.md`

```
# Line ~29:
# BEFORE: mcp__unimatrix__context_retrospective(feature_cycle: ...)
# AFTER:  mcp__unimatrix__context_cycle_review(feature_cycle: ...)
```

### `product/workflow/base-001/protocol-evolved/uni-agent-routing.md`

```
# Update context_retrospective reference to context_cycle_review
```

---

## Product Vision and README

### `product/PRODUCT-VISION.md`

Update all four occurrences (lines ~32, ~43, ~282, ~819).

### `README.md`

Update the tool table row for `context_retrospective` (line ~218):
```
# BEFORE: | context_retrospective | ... |
# AFTER:  | context_cycle_review  | ... |
```

### `product/ALPHA_UNIMATRIX_COMPLETED_VISION.md`

Update all references to `context_retrospective`.

### `CLAUDE.md`

Check for tool name in tool list. If present, update.

---

## Excluded Files (Historical Records — Do Not Update)

Per SPECIFICATION.md §SR-05, the following directories are deliberately excluded:
- `product/features/col-002/`, `col-002b/`, `col-009/`, `col-010/`, `col-010b/`, `col-012/`, `col-014/`, `col-016/`, `col-017/`, `col-020/`, `col-020b/`, `col-022/`
- `product/features/vnc-005/`, `vnc-008/`, `vnc-009/`, `vnc-011/`
- `product/features/nxs-008/`, `nxs-009/`
- `product/research/ass-007/`, `ass-014/`, `ass-015/`, `ass-016/`, `ass-018/`, `ass-020/`, `ass-022/`
- `product/features/crt-011/`, `crt-018/`, `crt-018b/`
- `product/features/bugfix-236/`
- `product/research/optimizations/`
- The current feature's own docs: `product/features/dsn-001/` (historical record of the design)

---

## Mandatory Pre-PR Verification

```
grep -r "context_retrospective" . --exclude-dir=.git
```

Must return zero results outside the excluded directories listed above. Any match
outside those directories is a missed update that must be fixed before merge.

---

## Key Test Scenarios

1. **Tool list contains context_cycle_review** (AC-13):
   - Python integration test `test_protocol.py` line ~55 asserts `"context_cycle_review"` is in tool list.
   - Negative assertion: `"context_retrospective"` is NOT in the tool list.

2. **context_cycle_review callable via Python harness** (AC-13, R-04):
   - `server.context_cycle_review(feature_cycle="col-022")` returns a valid structured response.
   - Confirms the rename propagated through the MCP router.

3. **classify_tool recognizes new name** (R-04):
   - `classify_tool("context_cycle_review")` returns `"other"` in `session_metrics.rs`.
   - `classify_tool("context_retrospective")` may return any value (it is no longer a valid tool name).

4. **Audit log strings updated** (R-04):
   - After calling `context_cycle_review`, the audit log contains `operation = "context_cycle_review"`.
   - No audit entry with `operation = "context_retrospective"` is generated.

5. **CycleParams.topic doc is domain-neutral** (AC-14):
   - Read `tools.rs`; assert "feature" does not appear as the primary example in the `topic` field doc.

6. **grep sweep passes** (R-04):
   - `grep -r "context_retrospective" .` returns zero matches outside excluded historical directories.

---

## Error Handling

This component has no runtime error paths — it is a pure rename. No new `Result` types.
The only failure mode is a missed update (R-04), guarded by the grep sweep.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for tool naming — no prior rename patterns found. This is the first tool rename in the repository.
- Deviations from established patterns: none. Rename follows SPECIFICATION.md §SR-05 exactly.
