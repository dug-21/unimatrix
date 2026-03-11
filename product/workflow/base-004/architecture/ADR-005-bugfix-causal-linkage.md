## ADR-005: Bugfix Causal Linkage via caused_by_feature Tag

### Context

When a bug is found and fixed, the bugfix protocol records an outcome tagged with the bugfix feature cycle (e.g., `bugfix/52-confidence-overflow`). But the root cause often originates in a prior feature's design decision (e.g., `crt-002` chose f32 for confidence, which later caused overflow). The knowledge link between the bug and its originating feature is lost -- future agents reviewing `crt-002`'s knowledge trail would not find the lesson.

The scope requires that "bugfix agents should identify what could have been done during design to prevent the bug, and link the outcome/rework to the feature that caused the issue."

### Decision

Add a `caused_by_feature` tag to bugfix outcomes and lessons:

1. The bug-investigator's stewardship section includes guidance: "If the root cause traces to a design decision from a prior feature, note the originating feature ID in your diagnosis report."

2. The bugfix protocol's Phase 5 (Outcome Recording) updates the `/record-outcome` call to include `caused_by_feature:{feature-id}` in the tags array when the investigator identified an originating feature.

3. Any `/store-lesson` call from a bugfix session includes the same `caused_by_feature:{feature-id}` tag.

This uses the existing tag mechanism -- no new fields, no schema changes. The tag format `caused_by_feature:{id}` is searchable via `context_search(tags: ["caused_by_feature:crt-002"])`.

### Consequences

- Future agents reviewing a feature's knowledge trail can discover bugs it caused. A retro for `crt-002` could find all lessons tagged `caused_by_feature:crt-002`.
- The tag is optional -- not all bugs trace to a specific prior feature. The investigator makes the judgment call. No validator enforcement on this tag.
- The retro quality pass (C5) can use `caused_by_feature` tags to surface patterns: "feature X caused 3 bugfixes -- its design decisions may need review."
- No Rust code changes. The tag is a string passed through existing `context_store` parameters.
- Risk: investigators may not always identify the originating feature accurately. This is acceptable -- an approximate link is better than no link. The retro can correct misattributions.
