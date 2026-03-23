# Agent Report: crt-028-agent-3-risk

## Output

- Produced: `/workspaces/unimatrix/product/features/crt-028/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count | Risk IDs |
|----------|-------|----------|
| Critical | 3 | R-01 (degradation boundary), R-02 (tail multiplier insufficiency), R-03 (SeekFrom::End N > file_len) |
| High | 5 | R-04 (non-canonical pairing), R-05 (reversal order), R-06 (UTF-8 truncation), R-07 (allowlist bypass), R-08 (quarantine filter removal) |
| Med | 4 | R-09 (key-param fallback secrets), R-10 (OQ-SPEC-1), R-11 (path traversal), R-12 (separator format) |
| Low | 1 | R-13 (crt-027 compile-time symbol rename) |

**Total minimum test scenarios**: 38 across 13 risks.

## Top Risks for Tester Attention

**R-01 (Critical)**: The degradation boundary in `extract_transcript_block` is structurally enforced by `Option<String>` return type but must be verified by test: pass a non-existent `transcript_path`, assert briefing is still written to stdout. Lesson #699 (silent None broke entire feedback loop) gives this historical weight.

**R-03 (Critical)**: `SeekFrom::End(-N)` when N > file size. The ADR-001 conditional guard (`if file_len > window`) must be tested at the exact boundary (`file_len = window`, `file_len = window + 1`, `file_len = 0`). Recommend the spec adopt the explicit clamp form (`let seek_back = window.min(file_len)`) to eliminate the class of bugs entirely.

**R-10 (Med, blocked)**: OQ-SPEC-1 — assistant turns with only `tool_use` + `thinking` blocks. Spec must resolve before tester can implement the scenarios. Recommendation: emit the pair when at least one `ToolPair` is present; suppress when both text and tool_use are absent.

## OQ-SPEC-1 Recommendation

Emit exchange pairs when at least one `ToolPair` is present (tool-use-only turns). Suppress only when both assistant text AND tool pairs are absent (pure-`thinking` turns). Suppressing tool-only turns produces false degradation in autonomous delivery runs where almost all assistant turns are tool calls.

## OQ-SPEC-2 Recommendation

Adopt the explicit clamp implementation over the conditional:
```rust
let seek_back = window.min(file_len);
if seek_back > 0 {
    file.seek(SeekFrom::End(-(seek_back as i64))).ok()?;
}
```
This handles `file_len = 0`, `file_len < window`, and `file_len >= window` without a separate branch for the small-file case.

## Non-Negotiable Tests (gate-blocking)

1. R-01: briefing written when transcript_path is non-existent (SR-07 / FR-06.7)
2. R-03: zero-byte file and exact-window-boundary seek (OQ-SPEC-2)
3. R-07: `sanitize_observation_source` unit test — all 6 cases (AC-11 / GH #354)
4. R-08: quarantine exclusion in `IndexBriefingService::index()` (AC-12 / GH #355)
5. R-10: OQ-SPEC-1 resolution test — tool-only assistant turn (blocked on spec clarification)

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned hook failures -- found entry #699 (silent None in hook pipeline, elevated R-01 to Critical)
- Queried: `/uni-knowledge-search` for risk patterns -- found entry #3331 (crt-028 WA-5 architecture, confirmed design alignment)
- Queried: `/uni-knowledge-search` for SQLite observation write site injection -- found entries #3242 (crt-027 source field ADR), #2745 (SQLite NOT NULL gotcha)
- Stored: nothing novel to store -- R-01/SR-07 degradation risk is captured in Lesson #699; no cross-feature pattern beyond what is already documented
