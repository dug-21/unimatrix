# Agent Report: vnc-016-agent-2b-spec

## Task

Update SPECIFICATION.md for vnc-016 to incorporate the production bug fix for the `UsageContext` trust-level gate in `usage.rs`, the updated integration test design using an enrolled Restricted+Write agent, and new acceptance criteria AC-10 through AC-13.

## Output

`product/features/vnc-016/specification/SPECIFICATION.md` — updated in place.

## Changes Made

### Objective
Extended to mention the production bug fix (trust-level gate silently drops cycle attribution for Restricted+Write agents).

### FR-04: UsageContext Gate Fix (NEW)
New functional requirement block covering:
- `write_capable: bool` field addition to `UsageContext` with no default
- Gate replacement in `record_mcp_usage` (lines 207–218)
- Gate replacement in `record_hook_injection` (lines 272–283)
- All other `UsageContext` construction sites set `write_capable: false`
- `context_store` handler sets `write_capable: true` (require_cap already verified)
- `trust_level` field is retained; only the feature-recording gate changes

### FR-05 (was FR-04): Positive-Path Integration Test — updated scenario
- Seven-step scenario expanded to nine steps
- Steps 2–3: generate unique `test_agent_id`; enroll via `context_enroll` as Restricted+Write
- Step 4: `context_store` for entry A uses `agent_id=test_agent_id` (not "human")
- FR-05.5 added: explains why this validates the fixed gate path (using "human" would mask the fix)

### FR-06 (was FR-05): Negative-Path Integration Test — minor clarification
- Note added that "human" is acceptable here because the purpose is confirming no false-positive, not gate validation

### FR-07 (NEW): Unit test for gate logic in `usage.rs`
- Pure unit tests verifying `write_capable: false` → `None` and `write_capable: true` → `Some(...)`
- No live store or MCP server required

### NFR-07, NFR-08 (NEW)
- NFR-07: `write_capable` has no default; enforced by Rust exhaustive struct construction
- NFR-08: `trust_level` field preserved; scope limited to feature-recording gate

### AC-02
- Updated step count reference from 7-step to 9-step scenario; references FR-05.2

### AC-10 through AC-13 (NEW)
- AC-10: `write_capable: bool` field present, no default
- AC-11: gate in both `record_mcp_usage` and `record_hook_injection` uses `ctx.write_capable`
- AC-12: `context_store` sets `write_capable: true`; all other sites set `write_capable: false`
- AC-13: unit test in `unimatrix-server` exercises both branches

### Domain Models
- "analytics write path" updated to explain the gate fix and the before/after behavior
- "write_capable" term defined
- "Restricted+Write agent" term defined

### User Workflows
- Workflow 3 renamed to clarify it applies to Privileged agents (always worked)
- Workflow 4 (NEW): Restricted+Write orchestrator agent
- Workflow 5 (NEW): gate logic unit test example

### Constraints
- C-01 step reference updated (step 2 → step 4)
- C-01b (NEW): Restricted+Write agent is required for the `context_store` call for entry A; "human" is prohibited for that step
- C-06 step reference updated
- C-11 (NEW): no default for `write_capable`
- C-12 (NEW): both gate blocks must be fixed (not just `record_mcp_usage`)
- C-13 (NEW): `write_capable: true` is unconditional within the `if let Some(fc)` branch

### Dependencies table
- `tools.rs` row expanded to cite `require_cap` at line 653 and UsageContext construction at ~826
- `usage.rs` row expanded to cite struct definition and both gate block line ranges

### NOT in Scope
- Three new exclusions added: `trust_level` removal, trust-level gating changes beyond the feature-recording block, trust escalation of the test agent

## Spec Decisions

1. **`write_capable` is set at the handler boundary, not derived from `trust_level` at record time.** The handler has already evaluated capability via `require_cap(Write)`. Setting `write_capable: true` in the handler is the appropriate enforcement point (ADR-007 pattern: enforce at the boundary, not inside the service).

2. **`trust_level` is retained on `UsageContext` without change.** Source reading confirms `trust_level` is present (line 60, `pub trust_level: Option<TrustLevel>`). The scope requires the gate fix only; no other consumer of `trust_level` is identified in scope.

3. **The `if let Some(fc) = usage_feature_cycle` guard in `tools.rs:822` means `write_capable: true` is only set inside a branch that already requires `feature_cycle` to be `Some`.** The `write_capable` field does not need to encode the presence/absence of `feature_cycle` — that is handled by the `Option<String>` field. The field solely carries the capability decision.

4. **The integration test positive path must use a Restricted+Write agent for the `context_store` call for entry A.** Using "human" (Privileged) would test a path that was never broken. The specification explicitly prohibits "human" for that step (C-01b) and explains why (FR-05.5).

5. **Both `record_mcp_usage` and `record_hook_injection` gates are identical in structure and must be fixed together.** Confirmed by reading lines 207–218 and 272–283. The spec requires both fixes (C-12); a half-fix is a latent bug.

## Constraints Discovered from Source Reading

- `UsageContext` struct at lines 50–76 of `usage.rs` already has `trust_level: Option<TrustLevel>` and `feature_cycle: Option<String>`. The `write_capable: bool` field is a pure addition; no existing field is removed.
- The `context_store` handler's `UsageContext` construction is guarded by `if let Some(fc) = usage_feature_cycle` at line 822. This means the construction site (and therefore `write_capable: true`) is only reached when `feature_cycle` is present — no additional conditional required.
- `require_cap(Capability::Write)` at tools.rs line 653 runs before the `UsageContext` construction. The `write_capable: true` assignment is therefore safe and unconditional within the `if let` branch.
- All `context_enroll` calls require Admin capability. The `human` agent has Admin capability in the infra-001 fixture and can enroll the test agent.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — lessons from id 4444 (vnc-015 AC-15 failure: missing agent_id on context_edge) and id 4411 (vnc-014 capability gate change) confirm the pattern: gate fixes require the Rust change AND an integration test exercising the fixed path with the correct agent class.
