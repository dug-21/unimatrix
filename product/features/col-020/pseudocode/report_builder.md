# C5: Report Builder (unimatrix-observe/src/report.rs)

## Purpose

Accommodate new fields on RetrospectiveReport. Per the architecture decision (C5 in ARCHITECTURE.md), the `build_report()` signature is **unchanged**. New fields are assigned via post-build mutation on the returned report, matching the existing pattern for `narratives` and `recommendations`.

## Code Change

The only change needed in `report.rs` is adding the five new fields (initialized to `None`) in the `build_report()` function's `RetrospectiveReport` struct literal.

```
// In build_report(), the returned RetrospectiveReport gains:
session_summaries: None,
knowledge_reuse: None,
rework_session_count: None,
context_reload_pct: None,
attribution: None,
```

This is a mechanical change driven by C2 (types). No algorithmic logic.

## Post-Build Mutation Pattern (reference for C6)

The handler already uses this pattern:

```rust
// Existing pattern in tools.rs:
let mut report = build_report(...);
report.recommendations = recommendations_for_hotspots(&report.hotspots);
report.narratives = Some(synthesize_narratives(&report.hotspots));
```

col-020 extends this:

```rust
// New assignments (in handler, documented here for context):
report.session_summaries = Some(summaries);
report.context_reload_pct = Some(reload_pct);
report.knowledge_reuse = Some(knowledge_reuse);
report.rework_session_count = Some(rework_count);
report.attribution = Some(attribution_metadata);
```

## Error Handling

No new error handling needed. The None initialization means if any upstream computation fails, the field stays None automatically.

## Key Test Scenarios

1. **build_report returns None for new fields**: Call build_report with existing parameters. Verify session_summaries, knowledge_reuse, rework_session_count, context_reload_pct, and attribution are all None.
2. **Post-build mutation works**: Call build_report, then assign session_summaries = Some(vec![...]). Verify the assignment sticks and serializes correctly.
3. **Existing tests pass unchanged (NFR-03)**: All existing report.rs tests continue to compile and pass with the five new None fields.
