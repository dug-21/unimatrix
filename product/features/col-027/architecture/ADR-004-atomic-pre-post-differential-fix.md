## ADR-004: Atomic Fix for Duplicated Pre-Post Differential in metrics.rs and friction.rs

### Context

The Pre-Post differential — the count of `PreToolUse` records minus the count of `PostToolUse`
records per tool — is independently implemented in two locations:

1. `unimatrix-observe/src/metrics.rs` `compute_universal()`: computes `permission_friction_events`
   as the sum of `(pre - post)` per tool.

2. `unimatrix-observe/src/detection/friction.rs` `PermissionRetriesRule::detect()`: computes
   per-tool retries as `pre_count.saturating_sub(post_count)` and fires a `HotspotFinding` when
   the value exceeds threshold 2.

Both implementations currently only count `"PostToolUse"` records in the terminal (denominator)
bucket. Because `PostToolUseFailure` was unregistered, this produced no records — but once
registered, each failure event will produce a `PreToolUse` with no `PostToolUse` counterpart,
inflating both the metric and the rule finding.

Unimatrix entry #3472 documents this as a pattern risk: the two sites must be updated atomically.
A partial fix (one without the other) would cause the metric and the rule to diverge — they would
report different signals from the same underlying data, which is confusing and would be flagged as
contradictory in downstream analysis.

The two sites cannot be trivially unified into a shared function because they have different outputs
(a scalar metric vs. per-tool findings) and different threshold logic. Unifying them is a
refactoring task not in scope for col-027.

### Decision

Both `compute_universal()` and `PermissionRetriesRule::detect()` must be updated **in the same
commit** to widen the terminal bucket:

**In `metrics.rs` `compute_universal()`:**
Widen the `post_counts` bucket to include both `hook_type::POSTTOOLUSE` and
`hook_type::POSTTOOLUSEFAILURE` records. The `permission_friction_events` formula becomes:
`sum over tools of pre.saturating_sub(post + failure)` — or equivalently, use a single `terminal`
counter that increments on either event type.

**In `friction.rs` `PermissionRetriesRule::detect()`:**
Rename the internal `post_counts: HashMap<String, u64>` to `terminal_counts` (making the intent
explicit without changing the rule name, finding category, or claim text). Increment `terminal_counts`
for both `"PostToolUse"` and `"PostToolUseFailure"` records. The `retries` formula becomes:
`pre_count.saturating_sub(terminal_count)`.

The rule name (`"permission_retries"`), category (`HotspotCategory::Friction`), severity
(`Severity::Warning`), and claim text are unchanged — renaming or recategorizing is deferred to
col-028.

**Commit enforcement:** The delivery agent must place both changes in a single commit. The
integration test for `PermissionRetriesRule` must include a case where `PostToolUseFailure` records
are present but pre/terminal counts are balanced, asserting no finding fires. The metrics test must
assert that `permission_friction_events == 0` when each `PreToolUse` is matched by either a
`PostToolUse` or a `PostToolUseFailure`.

### Consequences

**Easier:**
- `PermissionRetriesRule` will no longer fire for sessions where every Pre is matched by a
  PostToolUseFailure (tool failed, but was not cancelled/permission-blocked).
- `permission_friction_events` will no longer count tool failures as friction events.
- The two implementations remain consistent because they are fixed together.

**Harder / Watch for:**
- The existing test `test_permission_retries_exceeds_threshold` inserts 5 Pre records and 2 Post
  records, expecting `measured == 3.0`. After the fix, if any of the "missing" posts were
  `PostToolUseFailure`, the measured value would decrease. The test uses `make_pre` / `make_post`
  helpers and does not insert failure records, so it remains valid as-is. New tests must add a
  third helper `make_failure(ts, tool)` that produces an `ObservationRecord` with
  `event_type = "PostToolUseFailure"`.
- Future additions to the Pre-Post differential pattern should be captured in a shared helper or
  abstraction to prevent the two-site problem from recurring. This is a follow-on refactoring
  concern (not col-027 scope).
