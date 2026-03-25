## ADR-003: PostToolUseFailure Hook Type Is Not Normalized to PostToolUse

### Context

The `extract_observation_fields()` function in `listener.rs` normalizes `post_tool_use_rework_candidate`
events to `"PostToolUse"` for storage (introduced in col-019). This normalization exists because
rework candidates are a routing artifact of the hook dispatcher — they represent PostToolUse events
that were reclassified internally and should appear as regular PostToolUse records in the
observations table.

A question arises: should `PostToolUseFailure` be similarly normalized to `"PostToolUse"` before
storage? Arguments for normalization: detection rules that already count `"PostToolUse"` records
would automatically pick up failure events; no rule changes required.

Arguments against normalization: `PostToolUseFailure` is semantically distinct from `PostToolUse`.
A successful tool completion and a tool failure are different outcomes. Detection rules specifically
need to distinguish between them to implement the fixes required in col-027 (SR-07). Normalizing
would restore the original signal-loss problem: failures would be counted as successful completions
in the Pre-Post differential, partially fixing the count but destroying the failure signal.

The col-019 normalization is justified because `post_tool_use_rework_candidate` is a private
internal event type — not a Claude Code hook event. `PostToolUseFailure` is a first-class Claude
Code hook event with distinct semantics.

### Decision

`PostToolUseFailure` events are stored verbatim with `hook = "PostToolUseFailure"`. The
normalization block in `extract_observation_fields()` (which rewrites `"post_tool_use_rework_candidate"`
to `"PostToolUse"`) is NOT extended to include `PostToolUseFailure`.

Detection rules that need to treat failure events as terminal (i.e., to fix the Pre-Post
differential) do so by explicitly including `"PostToolUseFailure"` in their terminal bucket
logic — not by relying on the stored value being `"PostToolUse"`.

The `hook_type::POSTTOOLUSEFAILURE` constant (`"PostToolUseFailure"`) is the canonical reference
string for filtering failure records in detection rules and metrics.

### Consequences

**Easier:**
- Detection rules can independently query for failures (`event_type == hook_type::POSTTOOLUSEFAILURE`)
  vs. successes (`event_type == hook_type::POSTTOOLUSE`) — the stored data preserves both signals.
- `ToolFailureRule` can count `PostToolUseFailure` records directly without needing a secondary
  field to distinguish failure from success.
- Future rules (e.g., error classification, failure rate per phase) have the raw signal available.

**Harder / Watch for:**
- Any detection rule that uses `event_type == "PostToolUse"` to count terminal events will miss
  `PostToolUseFailure` unless explicitly updated. The blast-radius audit (ARCHITECTURE.md §Detection
  Rule Audit) covers all 21 rules and identifies the two that require updates. Rules not in that set
  correctly ignore failure events (their `"PostToolUse"` filters are intentional — they are looking
  for successful completions with response payloads).
- `response_size`-based metrics (`total_context_loaded_kb`, `edit_bloat_*`) filter on
  `hook_type::POSTTOOLUSE` by design. Because failure events have `response_size = None`, they would
  contribute zero even if the filter were widened. The explicit filter is still correct and should
  not be changed.
