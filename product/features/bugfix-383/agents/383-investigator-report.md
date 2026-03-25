# Investigator Report — bugfix-383

**Agent ID**: 383-investigator
**Date**: 2026-03-25

## Root Cause Analysis

`PermissionRetriesRule` in `crates/unimatrix-observe/src/detection/friction.rs` was misnamed from inception. The rule computes a Pre-Post terminal differential to detect unmatched tool invocations. Before col-027, the differential equalled tool failure counts because `PostToolUseFailure` was not a registered hook. col-027 fixed the computation by widening the terminal bucket to include `PostToolUseFailure` (ADR-004, atomic fix across `friction.rs` and `metrics.rs`). The rename was explicitly deferred to this issue.

Post-col-027, the rule detects **orphaned calls**: PreToolUse events with no matching terminal event (neither PostToolUse nor PostToolUseFailure). Causes include context overflow, parallel call cancellation, and interrupted subagent turns. This is distinct from `ToolFailureRule` which counts PostToolUseFailure events directly.

All user-visible identifiers still referenced "permission retries" — the struct name, rule_name string, claim text, recommendation text, and remediation text. This fix corrects all of them.

## What the Rule Measures Post-col-027

Tool invocations that started (PreToolUse fired) but received no terminal response — neither success nor failure. Causes: context overflow, parallel call cancellation, interrupted subagent turns.

## Affected Files

- `crates/unimatrix-observe/src/detection/friction.rs`
- `crates/unimatrix-observe/src/detection/mod.rs`
- `crates/unimatrix-observe/src/extraction/recurring_friction.rs`
- `crates/unimatrix-observe/src/metrics.rs`
- `crates/unimatrix-observe/src/report.rs`
- `crates/unimatrix-observe/src/synthesis.rs`
- `crates/unimatrix-observe/tests/detection_isolation.rs`
- `crates/unimatrix-server/src/mcp/response/retrospective.rs`
- `crates/unimatrix-server/src/mcp/tools.rs`
- `.claude/skills/uni-retro/SKILL.md`
- `packages/unimatrix/skills/retro/SKILL.md`

## Risk Assessment

Low. Computation logic unchanged. String identifiers only. Blast radius: worst case is silent missing recommendation — caught by the new contract test.

## Missing Test (Now Added)

`test_all_default_rules_have_non_fallback_recommendation_and_remediation` — contract test iterating all 22 rules, asserting each has a non-fallback recommendation and remediation.

## Knowledge Stewardship

Queried:
- Unimatrix #3446 (PermissionRetriesRule lesson — correction chain, confirmed col-027 deferred rename)
- Unimatrix #3419 (permission_friction_events pattern)
- Unimatrix #3477 (ADR-005 ToolFailureRule)
- Unimatrix #3476 (ADR-004 atomic fix)

Stored: nothing novel — orphaned-call semantics follow directly from existing lessons #3446 and #3476.
