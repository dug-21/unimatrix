# col-027 Implementation Brief: PostToolUseFailure Hook Support

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/col-027/SCOPE.md |
| Architecture | product/features/col-027/architecture/ARCHITECTURE.md |
| Specification | product/features/col-027/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-027/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-027/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| hook-registration | pseudocode/hook-registration.md | test-plan/hook-registration.md |
| hook-dispatcher | pseudocode/hook-dispatcher.md | test-plan/hook-dispatcher.md |
| core-constants | pseudocode/core-constants.md | test-plan/core-constants.md |
| observation-storage | pseudocode/observation-storage.md | test-plan/observation-storage.md |
| pre-post-differential-fix | pseudocode/pre-post-differential-fix.md | test-plan/pre-post-differential-fix.md |
| tool-failure-rule | pseudocode/tool-failure-rule.md | test-plan/tool-failure-rule.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component Map lists expected components from the architecture — actual file paths are filled during delivery. The Cross-Cutting Artifacts section tracks files that don't belong to a single component but are consumed by specific stages.

---

## Goal

Register and handle the `PostToolUseFailure` Claude Code hook event so that tool failures produce correctly-typed observation records, the Pre-Post differential used by `PermissionRetriesRule` and `permission_friction_events` is fixed to treat failure events as terminal (eliminating false friction findings present in every retrospective since nan-002), and a new `ToolFailureRule` detection rule surfaces genuine per-tool failure counts for future retrospectives.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Hook registration and dispatch pattern | Add `PostToolUseFailure` to settings.json with `matcher: "*"`; add explicit match arms in `build_request()` and `extract_event_topic_signal()`; never fall through to wildcard | SCOPE §Proposed Approach | architecture/ADR-001-posttoolusefailure-hook-registration-and-dispatch.md |
| Error field extraction | Add `extract_error_field()` as a sibling to `extract_response_fields()` that reads `payload["error"]` as a plain string; `PostToolUseFailure` arm must NOT call `extract_response_fields()` (which reads `tool_response` object and would silently return None) | SR-01 high risk | architecture/ADR-002-separate-error-field-extractor.md |
| No normalization of failure hook type | Store `hook = "PostToolUseFailure"` verbatim; do NOT normalize to `"PostToolUse"` the way `post_tool_use_rework_candidate` is normalized | ADR-003 | architecture/ADR-003-no-normalization-of-failure-hook-type.md |
| Atomic two-site differential fix | Both `metrics.rs compute_universal()` and `friction.rs PermissionRetriesRule` must be updated in the same commit; rename internal variable `post_counts` → `terminal_counts` in `PermissionRetriesRule` | SR-08 risk | architecture/ADR-004-atomic-pre-post-differential-fix.md |
| ToolFailureRule design | New rule in `friction.rs`, `rule_name = "tool_failure_hotspot"`, threshold 3 (strictly greater than), one finding per tool, registered in `default_rules()` | ADR-005 | architecture/ADR-005-tool-failure-rule-design.md |

---

## Files to Create or Modify

| File | Action | Summary |
|------|--------|---------|
| `.claude/settings.json` | Modify | Add `PostToolUseFailure` hook registration with `matcher: "*"` and command `unimatrix hook PostToolUseFailure` |
| `crates/unimatrix-core/src/observation.rs` | Modify | Add `pub const POSTTOOLUSEFAILURE: &str = "PostToolUseFailure"` to `hook_type` module; update `response_snippet` doc comment |
| `crates/unimatrix-server/src/uds/hook.rs` | Modify | Add explicit `"PostToolUseFailure"` match arm in `build_request()` and `extract_event_topic_signal()` |
| `crates/unimatrix-server/src/uds/listener.rs` | Modify | Add explicit `"PostToolUseFailure"` match arm in `extract_observation_fields()`; add new `extract_error_field()` function |
| `crates/unimatrix-observe/src/detection/friction.rs` | Modify | Fix `PermissionRetriesRule` to use `terminal_counts` (widened to include failures); add new `ToolFailureRule` struct; add `make_failure` test helper |
| `crates/unimatrix-observe/src/detection/mod.rs` | Modify | Register `ToolFailureRule` in `default_rules()`; update rule count doc comment from 21 to 22 |
| `crates/unimatrix-observe/src/metrics.rs` | Modify | Widen `post_counts` bucket in `compute_universal()` to include `hook_type::POSTTOOLUSEFAILURE` for `permission_friction_events` |

---

## Data Structures

### ObservationRecord (unimatrix-core/src/observation.rs) — unchanged struct, new constant

```rust
// hook_type module — add alongside PRETOOLUSE / POSTTOOLUSE / SUBAGENTSTART / SUBAGENTSTOPPED
pub const POSTTOOLUSEFAILURE: &str = "PostToolUseFailure";

// response_snippet doc comment update:
// Populated for PostToolUse (from tool_response object) and PostToolUseFailure (from error string).
```

### PostToolUseFailure Payload (from Claude Code hook)

```
{
  "tool_name": String,        // same field as PostToolUse
  "tool_input": Object,       // same field as PostToolUse — used for topic_signal
  "error": String,            // PLAIN STRING — not tool_response object
  "is_interrupt": bool?       // optional — absent if not user-interrupted
}
```

### extract_error_field return type (listener.rs — new function)

```rust
fn extract_error_field(payload: &serde_json::Value) -> (Option<i64>, Option<String>)
// Returns (None, Some(snippet)) where snippet = payload["error"].as_str() truncated to 500 chars
// Returns (None, None) if "error" field absent or null
// response_size is always None for failure events
```

### HotspotFinding produced by ToolFailureRule

```
rule_name:  "tool_failure_hotspot"
category:   HotspotCategory::Friction
severity:   Severity::Warning
claim:      "Tool '{tool_name}' failed {count} times"
measured:   count as f64
threshold:  3.0
evidence:   one EvidenceRecord per PostToolUseFailure event for the tool
              description: "PostToolUseFailure for {tool}"
              detail:      response_snippet (if present)
```

### PermissionRetriesRule internal variable rename

The internal `post_counts: HashMap<String, u64>` is renamed to `terminal_counts`. Both `"PostToolUse"` and `"PostToolUseFailure"` records increment `terminal_counts`. The differential is `pre_count.saturating_sub(terminal_count)`.

---

## Function Signatures

```rust
// unimatrix-server/src/uds/listener.rs
fn extract_error_field(payload: &serde_json::Value) -> (Option<i64>, Option<String>);

// unimatrix-observe/src/detection/friction.rs
pub struct ToolFailureRule;
impl DetectionRule for ToolFailureRule {
    fn name(&self) -> &'static str { "tool_failure_hotspot" }
    fn category(&self) -> HotspotCategory { HotspotCategory::Friction }
    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>;
}

// unimatrix-observe/src/detection/friction.rs — test module
fn make_failure(ts: u64, tool: &str) -> ObservationRecord;
// Produces ObservationRecord with event_type = "PostToolUseFailure", source_domain = "claude-code"

// unimatrix-core/src/observation.rs — hook_type module
pub const POSTTOOLUSEFAILURE: &str = "PostToolUseFailure";
```

---

## Constraints

1. **No schema migration.** The `observations.hook TEXT` column has no enum constraint; `"PostToolUseFailure"` as a value requires zero migration work.
2. **hook_type is string-based.** Per col-023 ADR-001, no `HookType` enum exists. Add `pub const` only; do not create or extend any enum.
3. **Fire-and-forget transport.** `PostToolUseFailure` must route to `HookRequest::RecordEvent`. No synchronous DB writes may be added to the dispatch path. The 40ms `HOOK_TIMEOUT` must not be exceeded.
4. **Hook must not fail.** The hook binary always exits 0. All field accesses use `.and_then()` / `.unwrap_or_default()` / Option chaining. Missing `tool_name`, `error`, `tool_input`, or `is_interrupt` must not panic.
5. **No stdout output.** `PostToolUseFailure` is observation-only; no injection path is needed.
6. **Do NOT call `extract_response_fields()` for failure events.** That function reads `payload["tool_response"]` (object) which does not exist on failure payloads; it would silently return `(None, None)`. The `"PostToolUseFailure"` arm must call only `extract_error_field()`.
7. **Atomic two-site fix.** `metrics.rs` and `friction.rs` differential fixes must ship in the same commit. A partial fix is not acceptable — the metric and the rule must remain consistent.
8. **No normalization.** Store `hook = "PostToolUseFailure"` verbatim. Do not rewrite to `"PostToolUse"`.
9. **No retroactive correction.** Prior `HotspotFinding` records from features before col-027 are not recomputed.
10. **No recommendation template changes.** `report.rs` allowlist recommendation text is addressed in col-026 (AC-19).
11. **`response_size = None` for failure records.** Error strings are small; do not populate `response_size`.
12. **Blast radius audit.** The full 21-rule audit is documented in ARCHITECTURE.md §Detection Rule Audit. Only `PermissionRetriesRule` and `compute_universal()` require changes; all other rules are confirmed no-action.

---

## Dependencies

**Internal crates:**
- `unimatrix-core` — adds `hook_type::POSTTOOLUSEFAILURE`
- `unimatrix-server` — adds dispatch and storage path for the new event type
- `unimatrix-observe` — fixes Pre-Post differential; adds `ToolFailureRule`

**Configuration:**
- `.claude/settings.json` — adds hook registration

**No external crates required. No schema migration tooling invoked.**

**Existing patterns reused (do not reinvent):**
- `make_pre` / `make_post` test helpers in `friction.rs` — extend with `make_failure`
- Fire-and-forget `RecordEvent` dispatch path in `hook.rs` — reused unchanged
- `extract_event_topic_signal()` in `hook.rs` — add `"PostToolUseFailure"` arm mirroring `"PostToolUse"`
- `truncate_at_utf8_boundary()` in `listener.rs` — reused for error string truncation in `extract_error_field()`

---

## NOT in Scope

- Retroactive correction of past retrospective findings (stored `HotspotFinding` records from prior features stay as-is)
- Error message classification (timeout vs. permission-denied vs. not-found) — raw snippet only
- Renaming `PermissionRetriesRule` rule name or finding category — deferred to col-028
- Renaming `permission_friction_events` metric field — only the computation is fixed
- Hook output injection — `PostToolUseFailure` is observation-only
- Allowlist recommendation text changes — addressed in col-026 AC-19
- Bash failure detection overlap — `is_bash_failure()` in `PostToolUse` rework handler is a separate path
- `data_quality_note` caveat in retrospective output for pre-col-027 features — accepted follow-on (SR-06)
- `ToolFailureRule` threshold configuration — hardcoded constant is fine for now (SR-05 accepted)
- `is_interrupt` field surfacing — captured in payload but not used by any rule or metric in col-027

---

## Alignment Status

Vision guardian review: **PASS on all 5 checks, 0 VARIANCEs, 0 FAILs.**

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — directly repairs W1-5 observation pipeline; restores behavioral signal integrity for W3-1 GNN training data |
| Milestone Fit | PASS — Collective-phase correctness fix; no future-milestone capabilities |
| Scope Gaps | PASS — all 7 SCOPE.md goals addressed by the source documents |
| Scope Additions | WARN (informational only, no approval needed) — `extract_error_field()` named function and `terminal_counts` rename are implementation details not listed verbatim in SCOPE.md; both are architecturally sound |
| Architecture Consistency | PASS — follows col-023 ADR-001 (string constants), fire-and-forget transport, no schema migration, two-site atomic fix correctly specified |
| Risk Completeness | PASS — 14 risks, all 8 scope risk items traceable, security section present, edge cases covered |

One informational note from the alignment report: SCOPE.md and ARCHITECTURE.md originally used `"tool_failures"` as the rule name; SPECIFICATION.md, RISK-TEST-STRATEGY.md, and ADR-005 all use `"tool_failure_hotspot"`. The synthesizer has confirmed the naming note in the spawn prompt: **use `"tool_failure_hotspot"` throughout**. SPECIFICATION.md is authoritative for implementation.

---

## Critical Risk Summary for Implementers

The two highest-priority risks from RISK-TEST-STRATEGY.md:

**R-01 (Critical):** The `"PostToolUseFailure"` arm in `extract_observation_fields()` must call `extract_error_field()`, not `extract_response_fields()`. Calling the wrong function returns `(None, None)` silently — the error content is lost with no compile error.

**R-02 (Critical):** `metrics.rs` and `friction.rs` must be updated in the same commit. The R-02 test must assert that both `compute_universal()` and `PermissionRetriesRule::detect()` agree on the same observation set within a single test function.

Test baseline: 2169 unit + 16 migration + 185 infra integration. All new tests are additive; no existing tests may be deleted or modified.
