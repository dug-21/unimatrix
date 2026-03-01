# Scope Risk Assessment: col-002

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | ISO-8601 manual timestamp parsing without chrono/time crate may silently produce incorrect results for edge-case formats or timezone variations | Med | Med | Architect should constrain the timestamp format to a single canonical pattern emitted by hook scripts and test exhaustively with boundary dates |
| SR-02 | JSONL files in `~/.unimatrix/observation/` grow unbounded during long sessions; a single session with thousands of tool calls could produce very large files that slow analysis | Med | Med | Architect should consider per-record size caps (response_snippet truncation) and document expected file sizes from ASS-013 baseline data |
| SR-03 | New `unimatrix-observe` crate adds a workspace member but must remain independent of `unimatrix-store` and `unimatrix-server` -- accidental coupling through shared types could break this boundary | High | Med | Architect should define a clear type boundary: observe crate owns its own record and report types; server crate handles conversion |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Scope explicitly defers baseline comparison to col-002b but the MetricVector struct must be extensible enough to support it without breaking serialization | Med | Low | Architect should ensure MetricVector uses serde with `#[serde(default)]` on fields that col-002b will add |
| SR-05 | 3 detection rules may be insufficient to validate the framework's extensibility claim (AC-18) -- if the trait design only accommodates these 3, col-002b will require framework rework | High | Low | Architect should design the rule trait with col-002b's full 21-rule set in mind, verifying the interface supports all 4 categories |
| SR-06 | Hook scripts are shell scripts that cannot be tested within the Rust test suite -- testing gaps in the collection layer could cause silent data loss | Med | Med | Spec writer should define integration test patterns using synthetic JSONL files to validate end-to-end pipeline without depending on hook execution |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Adding OBSERVATION_METRICS as 14th table to `Store::open` changes the database initialization path used by all existing tests | Med | Low | Architect should verify table addition follows the same pattern as OUTCOME_INDEX (col-001) -- open in write txn, no schema version bump |
| SR-08 | `context_status` extension adds observation fields to `StatusReport` -- existing tests that construct `StatusReport` will need updating | Low | High | This is expected churn, not a risk. Spec writer should note it as a known test impact |
| SR-09 | Content-based feature attribution depends on consistent file path conventions (`product/features/{id}/`) -- if agents use relative paths or abbreviated references, attribution accuracy degrades | Med | Med | Architect should document the attribution signal priority and define fallback behavior for unattributable sessions |

## Assumptions

- **Hook API stability**: SCOPE.md assumes Claude Code hooks deliver PreToolUse, PostToolUse, SubagentStart, SubagentStop with `session_id`, `tool_name`, `tool_input`, and `tool_response` fields. If the hook API changes, the collection layer breaks. (SCOPE.md "Background Research > Claude Code hooks")
- **Single-project scope**: The observation directory (`~/.unimatrix/observation/`) is global. Multi-project use would intermingle session files from different projects. SCOPE.md explicitly marks multi-project as a non-goal. (SCOPE.md "Non-Goals")
- **JSONL format control**: Because hook scripts are authored by this project, the JSONL record format is fully controlled. No external producers. (SCOPE.md "Goal 1")

## Design Recommendations

- **SR-01, SR-03**: Define a `Timestamp` newtype in the observe crate that parses exactly one format. Keep all observation types in `unimatrix-observe` to enforce the crate boundary.
- **SR-05**: Design the detection rule trait by examining all 21 rules in the SCOPE.md reference table. Validate the trait handles per-tool metrics (permission retries), per-session metrics (timeout), per-command regex (sleep), and future per-agent/per-phase metrics.
- **SR-07**: Follow the OUTCOME_INDEX precedent exactly -- add table in `db.rs`, update comment count, add to test.
