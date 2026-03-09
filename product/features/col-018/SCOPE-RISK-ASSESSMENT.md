# col-018: Scope Risk Assessment

## SR-01: Observation write failure silently drops prompt data

**Likelihood**: Low
**Impact**: Medium
**Category**: Data loss

The observation write is fire-and-forget (`spawn_blocking` with no `.await`). If `insert_observation()` fails (e.g., disk full, SQLite lock contention), the prompt observation is silently lost. The search response is still returned successfully, so the user sees no error.

**Mitigation**: This is the existing pattern for all observation writes (col-012). The `tracing::error!` log on failure is already present. No new mitigation needed beyond what col-012 established. Acceptable risk for a non-critical data path.

## SR-02: Topic extraction from short/vague prompts produces noise

**Likelihood**: Medium
**Impact**: Low
**Category**: Signal quality

User prompts like "fix it" or "continue" will pass through `extract_topic_signal()` which looks for feature ID patterns. Short prompts without feature IDs will correctly return `None` for topic_signal. However, prompts containing incidental feature-ID-like patterns (e.g., "use rust-123 crate") could produce false positives.

**Mitigation**: This is the same risk profile as col-017's hook-side extraction. The `is_valid_feature_id()` validator requires a hyphen and safe characters. Majority vote in session-level attribution (col-017) smooths out individual false positives. No additional mitigation needed.

## SR-03: Input truncation inconsistency with other observation types

**Likelihood**: Low
**Impact**: Low
**Category**: Data consistency

The `extract_observation_fields()` function in listener.rs handles input extraction differently per event type (tool_input for PreToolUse, command for Bash, etc.). The new UserPromptSubmit observation constructs its `ObservationRow` directly in the ContextSearch dispatch arm, not through `extract_observation_fields()`. If future changes add truncation or normalization to `extract_observation_fields()`, the ContextSearch path could diverge.

**Mitigation**: Document the direct construction in code comments. The prompt text is the `query` string which is already bounded by `MAX_PAYLOAD_SIZE` (1 MiB) at the wire protocol level. Consider truncating `input` to a reasonable limit (e.g., 4096 chars) to match the spirit of other observation types.

## SR-04: Session ID None edge case

**Likelihood**: Very Low
**Impact**: Low
**Category**: Edge case

The `ContextSearch` request has `session_id: Option<String>`. While the hook always populates this field (hook.rs:261), a malformed or manually-constructed UDS request could omit it. The observation would need a session_id for the NOT NULL constraint on the observations table.

**Mitigation**: Use a fallback session_id (e.g., "unknown") when None, or skip the observation write entirely when session_id is missing. The latter is cleaner -- no session means the event is not from a normal hook flow and observation recording is not meaningful.

## SR-05: Duplicate observation for empty-prompt fallback

**Likelihood**: Very Low
**Impact**: Low
**Category**: Correctness

Empty-prompt UserPromptSubmit goes through `generic_record_event()` on the hook side, producing a `RecordEvent` that gets an observation via the existing RecordEvent dispatch path. The ContextSearch dispatch arm is not reached for empty prompts (hook.rs:255-257 returns early). No duplication risk.

**Mitigation**: None needed. The code paths are mutually exclusive by design.

## Risk Summary

| ID | Risk | L | I | Architect attention |
|----|------|---|---|---------------------|
| SR-01 | Silent observation write failure | Low | Med | Fire-and-forget is established pattern; acceptable |
| SR-02 | Topic noise from short prompts | Med | Low | Same risk as col-017; majority vote mitigates |
| SR-03 | Input truncation divergence | Low | Low | Add truncation limit; document direct construction |
| SR-04 | Session ID None edge case | VLow | Low | Skip observation when session_id is None |
| SR-05 | Duplicate observation risk | VLow | Low | Code paths mutually exclusive; no risk |

## Top 3 Risks for Architect Attention

1. **SR-03**: Input truncation -- architect should decide on a truncation limit for prompt text in the observation `input` field.
2. **SR-04**: Session ID None handling -- architect should decide skip-vs-fallback behavior.
3. **SR-01**: Silent failure is acceptable given established patterns, but architect should confirm.
