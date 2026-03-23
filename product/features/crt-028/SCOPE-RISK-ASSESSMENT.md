# Scope Risk Assessment: crt-028

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Transcript JSONL format is Claude Code-controlled and undocumented internally — schema could change silently across Claude Code versions, breaking extraction | High | Med | Architect must design extraction to fail-open on unknown record shapes, not panic or error; all unknown `type` values must be skipped, not rejected |
| SR-02 | Reverse-scan over a large JSONL file (long session, many tool calls) may read megabytes before finding k pairs — sync I/O on the hook path with no size guard | Med | Med | Architect should cap the byte budget of the file scan (e.g., only read the last N bytes before parsing), not just cap output; reading the full file for a 1MB transcript before discarding is a latency risk on the sub-50ms hook path |
| SR-03 | `MAX_PRECOMPACT_BYTES` (~3000) is a compile-time constant with no runtime override — if the value is wrong for a given model's context window, agents have no way to tune it without a code change | Low | Low | Architect should note the constant location and document it as a tunable in `config.toml` scope for a future config pass; it need not be configurable in this feature but should not be buried |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | The transcript prepend happens in `write_stdout` (or a local helper) — the exact insertion point (before vs. after `BriefingContent`) is settled as D-5, but if `BriefingContent.content` is empty the output could be transcript-only with no section separator | Low | Med | Spec should define exact output format when briefing content is empty: transcript block still emitted, separator present, no merged/ambiguous block |
| SR-05 | GH #354 fix (allowlist `source` field in `listener.rs`) is a security patch bundled with a new feature — risk that reviewer scrutiny focuses on the new extraction logic and under-reviews the allowlist change | Med | Med | Spec writer should call out GH #354 as a standalone security AC with its own explicit test assertion; do not bury it in the general AC table |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Hard dependency on crt-027 (`IndexBriefingService`, `IndexEntry`, `format_index_table`) — if crt-027 merges with API changes after crt-028 design is complete, the `write_stdout` integration point may need rework | Med | Low | Architect must pin the exact crt-027 contract surface used (method signatures, struct fields) and flag any deviation as a breaking change requiring re-scoping |
| SR-07 | Lesson #699: hardcoded `None` in the hook pipeline silently broke the entire feedback loop with no test failure. The graceful-degradation mandate (D-6, AC-07/08) for transcript read failures carries the same risk — a mis-scoped `?` or early return could silently skip injection in non-failure cases | High | Med | Architect must design the degradation boundary precisely: only the transcript block is skipped on failure; `BriefingContent` must still be written. Spec should include an explicit test for: failure path produces non-empty stdout (briefing still written) |

## Assumptions

- **SCOPE.md §Background Research / Transcript JSONL format**: Assumes `transcript_path` points to a file that is complete and intact at PreCompact hook time. If Claude Code writes the transcript asynchronously and flushes after firing PreCompact, the last N lines may be absent. This assumption is not verified in the scope — it cites "PreCompact fires before compaction; file is intact" without evidence of flush ordering.
- **SCOPE.md §Constraints / No tokio runtime**: Assumes sync I/O is sufficient for the file read latency budget. No measurement of typical transcript file sizes is provided; the 50ms budget may be violated for long sessions.
- **SCOPE.md §Settled Design Decisions / D-3**: Assumes `~300 bytes` is enough context for a tool result to be useful. This is an UX assumption, not validated — if the snippet is too short, restoration quality degrades silently.

## Design Recommendations

- **SR-01 + SR-07**: The extraction parser and the degradation path are both high-risk. Architect should treat them as separate concerns in the implementation — one function for parsing (fails open), one for budget-filling (always returns even if empty), one for the write path (always writes briefing regardless). This decomposition makes each part independently testable.
- **SR-02**: Add a `seek_from_end` or `tail -bytes` equivalent before the JSONL parse. Reading only the last `MAX_PRECOMPACT_BYTES * 4` bytes (a reasonable multiplier for raw-to-extracted ratio) before parsing avoids full-file I/O and keeps the hook within budget.
- **SR-05**: GH #354 security fix should have its own dedicated test in the spec, not share coverage with the broader transcript extraction tests. The allowlist behavior (unknown value → fallback, not error) is a security property.
- **SR-06**: Spec writer should reference the exact crt-027 symbols consumed (`IndexBriefingService::index`, `format_index_table`) and note that any renaming in crt-027 post-merge is a breaking change to crt-028.
