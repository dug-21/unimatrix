## ADR-005: ToolFailureRule — Detection Rule for Per-Tool Failure Counts

### Context

Once `PostToolUseFailure` events are stored, the retrospective pipeline can observe tool failure
patterns. `PermissionRetriesRule` is being fixed to exclude failure events from the retries count,
which means genuine tool failure rates would otherwise become invisible to the detection layer.

The question is how to surface tool failure signal: extend an existing rule, add a new rule, or
defer to a future feature.

Option A — extend `PermissionRetriesRule` to also report failures: The rule's name and semantics
are already misaligned (per Unimatrix entry #3446). Adding failure counting to it would further
conflate two distinct signals.

Option B — add `ToolFailureRule` in `friction.rs` alongside `PermissionRetriesRule`: A new rule
has a clear name, single responsibility, and its own threshold. Fits the existing rule registration
pattern in `mod.rs`.

Option C — defer to col-028: Acceptable if implementation complexity is high, but col-027 has
the data in place and the rule itself is straightforward. Deferring loses the diagnostic benefit
immediately after the hook is registered.

The threshold value of 3 was determined in SCOPE.md. No configuration path is needed for col-027;
if threshold tuning emerges as a need, it can be addressed as a follow-on (SR-05 accepted as-is).

### Decision

Add `ToolFailureRule` as a new struct implementing `DetectionRule` in
`unimatrix-observe/src/detection/friction.rs`, registered in `mod.rs` alongside existing rules.

Specification:
```
rule_name:  "tool_failure_hotspot"
category:   HotspotCategory::Friction
severity:   Severity::Warning
threshold:  3  (constant — fires when count > 3, i.e., strictly more than 3)
```

Implementation pattern:
- Pre-filter to `source_domain == "claude-code"` records (consistent with all friction rules)
- Count `PostToolUseFailure` records per `tool` (using `record.tool.as_ref()`)
- For each tool where `count > threshold`: emit one `HotspotFinding` with:
  - `claim`: `"Tool '{tool}' failed {count} times"`
  - `measured`: `count as f64`
  - `threshold`: `3.0`
  - Evidence records: each `PostToolUseFailure` record for that tool as an `EvidenceRecord`
    with description `"PostToolUseFailure for {tool}"` and `detail: response_snippet` (if present)

The rule fires **one finding per tool** exceeding threshold — not one aggregate finding. This
matches the per-tool pattern of `PermissionRetriesRule`.

Test helpers: add `make_failure(ts: u64, tool: &str) -> ObservationRecord` to the test module in
`friction.rs`, producing a record with `event_type = "PostToolUseFailure"`. Tests must cover:
- No finding when all tools have ≤ 3 failures
- One finding per tool exceeding threshold when multiple tools fail
- Finding measured value equals failure count
- Records from non-claude-code domains are excluded

### Consequences

**Easier:**
- Retrospectives will surface "Tool X failed N times" findings immediately after col-027 is
  deployed, using real `PostToolUseFailure` records from new sessions.
- The rule is independently testable with the `make_failure` helper.
- The name `"tool_failure_hotspot"` is unambiguous — no confusion with `"permission_retries"` — and self-describing for retrospective consumers.

**Harder / Watch for:**
- Sessions before col-027 deployment have no `PostToolUseFailure` records (the hook was not
  registered). `ToolFailureRule` will produce no findings for those sessions regardless of how
  many tool failures actually occurred. This is expected and correct forward-only behavior.
- The threshold of 3 is a single constant. Tools that are inherently flaky (e.g., external API
  calls) may trigger the rule legitimately. If threshold tuning is needed, the constant should be
  extracted to a named constant `TOOL_FAILURE_THRESHOLD` at the top of `friction.rs` to make it
  easy to find and adjust — even before a configuration path is added.
- The `detail` field in `EvidenceRecord` uses `response_snippet`, which is the truncated `error`
  string. For tools with long error messages, the detail will be truncated at 500 characters. This
  is intentional and consistent with how `extract_error_field()` stores the snippet.
