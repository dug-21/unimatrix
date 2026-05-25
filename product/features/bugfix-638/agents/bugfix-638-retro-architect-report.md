# Retrospective Architect Report: bugfix-638

## Stewardship Review

No entries were stored during the bugfix-638 session itself (all three agents deferred or declined storage). Four existing entries required correction based on the shipped fix.

### Corrected Entries

| Original | Replacement | Reason |
|----------|-------------|--------|
| #3465 | #4597 | Procedure missing binary crate target guarantee and daemon-child log file writer |
| #3468 | #4598 | ADR missing binary vs library crate target mismatch and .add_directive pattern |
| #3461 | #4602 | Obs pattern prerequisite section stale — didn't reflect #638 binary crate target guarantee |

### Validated (No Change Needed)

| Entry | Status |
|-------|--------|
| #4569 (ADR-001 foreground mode) | Validated — "zero blast radius" consequence held; daemon-child changes were additive as predicted |
| #3453 (with_env_filter(&str) lesson) | Still accurate and relevant |
| #1939 (nix setsid process feature) | Still accurate |

## New Entries Stored

| ID | Type | Title |
|----|------|-------|
| #4599 | lesson-learned | Rust workspace binary vs library crate: RUST_LOG directive-style filters silently exclude the binary crate target |
| #4600 | pattern | Daemon child: open log file explicitly with Mutex<LineWriter<File>>, mandatory stderr fallback on failure |
| #4601 | lesson-learned | Bugfix investigation can diagnose a real fragility that is not the active production cause — verify with production environment variables |

## Hotspot-Derived Observations

- **35 compile cycles**: Consistent with the daemon tracing init being a complex conditional block with multiple writer types. No procedure improvement identified — the changes involved type-level branching (Mutex<LineWriter<File>> vs stderr) that requires incremental verification.
- **276 KB context load before first write**: 4.0 sigma outlier. The investigator loaded substantial codebase context (main.rs is 1550 lines) before diagnosing. Acceptable for a bug in a large entrypoint function.
- **14 sleep workarounds**: The integration test spawns a real binary and sleeps 3s for startup. This is inherent to process-spawn integration testing. No improvement — `run_in_background` is not applicable to test code.
- **26.4% search-via-bash**: Not actionable for this session — investigator needed git and cargo commands.

## Summary

| Category | New | Updated | Skipped | Deprecated |
|----------|-----|---------|---------|------------|
| Patterns | 1 (#4600) | 1 (#3461->#4602) | 0 | 0 |
| Procedures | 0 | 1 (#3465->#4597) | 0 | 0 |
| ADRs | 0 | 1 (#3468->#4598) | 0 | 0 |
| Lessons | 2 (#4599, #4601) | 0 | 0 | 0 |
