# Security Review: bugfix-389-security-reviewer

## Risk Level: low

## Summary

This fix adds a missing upstream step: `build_cycle_event_or_fallthrough` in `hook.rs` now extracts the `goal` field from `tool_input` for `CycleType::Start` events, truncates it at `MAX_GOAL_BYTES` using a correct UTF-8 char-boundary walk, and inserts it into the `RecordEvent` payload. The listener's existing `payload.get("goal")` read path was already correct; only the upstream insertion was missing. The production code change in `listener.rs` is limited to an import reorder — all new listener code is tests. No new dependencies, no new trust boundaries, no new external inputs, no secrets.

## Findings

### Finding 1: Double truncation is safe and idempotent
- **Severity**: informational
- **Location**: `hook.rs:632-649` (new block), `listener.rs:2443-2456` (pre-existing)
- **Description**: The goal string is truncated in `hook.rs` at `MAX_GOAL_BYTES`, then the listener truncates again at the same constant. Because the hook path runs first and the already-truncated string is at most `MAX_GOAL_BYTES` bytes, the listener's truncation is always a no-op — `g.len() <= max_bytes` on the fast path and no further allocation occurs. The behavior is idempotent and correct. A note: the listener will also emit a `tracing::warn!` if the goal exceeds the limit — but since the hook already truncated, this warn can never fire on the hook-sourced path. The warn remains useful for the direct-MCP path (context_cycle called directly, not via hook), so it is not dead code overall.
- **Recommendation**: No action required. If future maintainers find the double-guard confusing, a doc comment on the listener block noting "already bounded by hook.rs; guard here covers direct-MCP path" would clarify intent.
- **Blocking**: no

### Finding 2: Empty string stored verbatim (by design, but worth naming)
- **Severity**: informational
- **Location**: `hook.rs:632-649`, `listener.rs:2434`
- **Description**: If `tool_input["goal"]` is the empty string `""`, it passes through both truncation guards and is stored as an empty string in the registry and DB. The listener comment explicitly documents this: "UDS path: no whitespace or empty-string normalization (ADR-005 FR-11 scope = MCP only)." This is intentional per ADR-005 — normalization is enforced only on the MCP path. The hook path stores what it receives.
- **Recommendation**: The design decision is documented. No action required. Consumers of `current_goal` should treat `Some("")` the same as `None` if needed (this is a call-site concern, not a security concern).
- **Blocking**: no

### Finding 3: `goal` value is user-controlled and flows into SQLite via parameterized bind
- **Severity**: low (no finding)
- **Location**: `db.rs:325-340`, `listener.rs:2527`
- **Description**: The goal string originates from `tool_input["goal"]` — a user-controlled JSON string field. It is inserted into SQLite via `sqlx::query(...).bind(goal)`. SQLite parameterized binding (`?N`) prevents SQL injection; the value is bound as a typed parameter, never string-interpolated into the query. No SQL injection risk exists.
- **Recommendation**: Confirmed safe. No action required.
- **Blocking**: no

### Finding 4: `goal` is inserted into a `serde_json::Value` payload and serialized over UDS
- **Severity**: low (no finding)
- **Location**: `hook.rs:668-670`, `wire.rs:266-283`
- **Description**: The goal is inserted via `payload["goal"] = serde_json::Value::String(g.clone())`. JSON string values are serialized with proper escaping by serde_json; there is no injection vector into the JSON wire frame. The UDS transport has a `MAX_PAYLOAD_SIZE` (1 MiB) guard at the frame level. A 1024-byte goal cannot inflate the total payload beyond any meaningful limit.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 5: `eprintln!` in hook.rs vs `tracing::warn!` in listener.rs — not a bug, documented by design
- **Severity**: informational
- **Location**: `hook.rs:638`
- **Description**: The new hook.rs code uses `eprintln!` rather than `tracing::warn!` for the truncation log message. This is explicitly documented in the comment: "Uses eprintln! (not tracing!) — hook runs outside the tokio runtime (ADR-002)." Using tracing in a context with no active tracing subscriber (the hook subcommand runs synchronously, no tokio runtime) would panic or silently drop the event. `eprintln!` is the correct choice per the established ADR.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 6: Unconditional `set_current_goal(None)` on cycle_start without goal key
- **Severity**: low
- **Location**: `listener.rs:2460`
- **Description**: When a `cycle_start` event arrives with no `goal` key in the payload, `set_current_goal` is called with `None`, unconditionally clearing any previously set goal. This is called out in test T-389-03 and the test comment explicitly documents: "set_current_goal is unconditional — None resets current_goal (no guard)." The behavior is intentional and consistent with how `set_current_phase` works. The security implication is that a duplicate `cycle_start` without a goal key clears the goal for the session. Since the hook fires on every `context_cycle(type=start)` call, this means a caller who issues `context_cycle(type=start)` without a goal after one with a goal will reset the goal. This is the intended contract but is worth confirming: the PR author has explicitly acknowledged and pinned this behavior in T-389-03.
- **Recommendation**: No change required. The behavior is intentional and tested.
- **Blocking**: no

### Finding 7: `tool_name.contains("context_cycle")` match — pre-existing, not introduced by this PR
- **Severity**: informational (pre-existing)
- **Location**: `hook.rs:573-582`
- **Description**: The existing `R-09 mitigation` guards against name collision by requiring either `tool_name == "context_cycle"` (exact match) or `tool_name.contains("unimatrix")`. This is pre-existing code not changed by this PR. The new goal extraction only executes after this guard passes, so it inherits the same trust boundary as the rest of `build_cycle_event_or_fallthrough`. No regression here.
- **Recommendation**: Not introduced by this PR; pre-existing design. No action required.
- **Blocking**: no

## Blast Radius Assessment

The worst case if this fix has a subtle regression:

1. **Goal extraction fails silently**: The `and_then(|v| v.as_str())` chain returns `None` if `tool_input["goal"]` is not a JSON string (e.g., a number or object). In that case, `goal_opt` is `None`, the payload key is absent, the listener reads `None`, and `set_current_goal(None)` is called. Result: goal not set — same as the pre-fix state (the bug). This is a safe degradation.

2. **Truncation bug**: If the UTF-8 char-boundary walk had an off-by-one, the goal could be truncated one byte shorter than expected. At worst, a multi-byte character at the boundary is dropped. This is not data corruption — the goal is metadata for briefing injection. No data integrity failure.

3. **Payload key collision**: If some other code path already sets `payload["goal"]` before the new insertion, the new insertion silently overwrites it. Inspecting the pre-existing payload construction (lines 635-647): only `feature_cycle`, `phase`, `outcome`, and `next_phase` are set before the new insertion. `goal` was not a key in that set. No collision possible.

4. **Session registry pollution**: If `set_current_goal` is called with an oversized or malformed string, it is stored in an in-memory `HashMap<String, SessionState>`. The value is already bounded to `MAX_GOAL_BYTES` before it reaches the registry. Worst case: 1024 bytes per session × number of sessions. Not a meaningful memory concern.

Blast radius is **narrow and self-contained**: failure modes are limited to the goal not being set (pre-fix state) or being silently truncated. No cascading failure, no data corruption, no privilege escalation path.

## Regression Risk

**Low.** The change adds a new code path that was entirely absent before. Existing behavior for `phase-end` and `stop` events is unchanged — the new block explicitly yields `None` for non-Start cycle types. The payload construction for `phase`, `outcome`, and `next_phase` fields is unmodified. The only new behavior is: `goal` key appears in the `RecordEvent` payload for `cycle_start` events when supplied.

The production code change in `listener.rs` is an import reorder only (cosmetic, no behavioral change). All listener behavior added is in tests.

Pre-existing tests that cover `build_cycle_event_or_fallthrough` for `phase-end` and `stop` paths remain valid — the new code explicitly checks `cycle_type == CycleType::Start` before extracting the goal.

## Dependency Safety

No new dependencies introduced. `serde_json::Value::String`, `MAX_GOAL_BYTES`, and the char-boundary walk are all existing constructs in the codebase.

## Secrets Check

No hardcoded secrets, API keys, tokens, or credentials in the diff. The `goal` field is user-supplied metadata.

## PR Comments

- Posted 1 comment on PR #390.
- Blocking findings: no.

## Knowledge Stewardship

Nothing novel to store — the double-truncation idiom (hook truncates for latency, listener truncates as a safety net covering the direct-MCP path) is specific to this PR's architecture and not a generalizable anti-pattern. The pattern of explicitly documenting unconditional write semantics in tests (T-389-03) is good practice already present in the codebase.
