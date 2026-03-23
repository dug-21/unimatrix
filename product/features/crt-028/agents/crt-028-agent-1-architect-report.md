# Agent Report: crt-028-agent-1-architect

## Status: COMPLETE

## Output Files

- `/workspaces/unimatrix/product/features/crt-028/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/crt-028/architecture/ADR-001-tail-bytes-transcript-read.md`
- `/workspaces/unimatrix/product/features/crt-028/architecture/ADR-002-tool-use-result-pairing.md`
- `/workspaces/unimatrix/product/features/crt-028/architecture/ADR-003-graceful-degradation-contract.md`
- `/workspaces/unimatrix/product/features/crt-028/architecture/ADR-004-source-field-allowlist.md`

## Unimatrix Entries

- #3333 — ADR-001: Tail-Bytes Read Strategy
- #3334 — ADR-002: Tool-Use/Tool-Result Pairing
- #3335 — ADR-003: Graceful Degradation Contract
- #3336 — ADR-004: Source Field Allowlist (GH #354)

## Key Decisions

1. **Tail-bytes read** (ADR-001): seek to last 12,000 bytes (MAX_PRECOMPACT_BYTES * 4) before JSONL parse. Bounds I/O to ~1ms regardless of session length. First line after mid-file seek silently discarded by fail-open parser.

2. **Adjacent-record pairing** (ADR-002): tool_use/tool_result matched by looking ahead one JSONL record. Correct for Claude Code's canonical structure. Unmatched pairs emit empty snippet — degradation, not error.

3. **Degradation boundary** (ADR-003): `extract_transcript_block() -> Option<String>` is the sole envelope. All failures → `None`. BriefingContent always written. Structurally enforced by return type — no Result, no unwrap().

4. **Source allowlist** (ADR-004): `sanitize_observation_source()` helper with exact match on `{"UserPromptSubmit", "SubagentStart"}`. No length cap needed — allowlist exhausts all valid values. Single update point for future source types.

## Three Functions (hook.rs)

| Function | Signature | Role |
|----------|-----------|------|
| `extract_transcript_block` | `fn(path: &str) -> Option<String>` | Tail-read + parse + format; all failures → None |
| `build_exchange_pairs` | `fn(lines: &[&str]) -> Vec<ExchangeTurn>` | JSONL parse, adjacent pairing, reverse order |
| `prepend_transcript` | `fn(transcript: Option<&str>, briefing: &str) -> String` | Combine blocks with section separators (SR-04) |

## New Constants (hook.rs)

| Constant | Value |
|----------|-------|
| `MAX_PRECOMPACT_BYTES` | 3000 |
| `TAIL_MULTIPLIER` | 4 |
| `TOOL_RESULT_SNIPPET_BYTES` | 300 |
| `TOOL_KEY_PARAM_BYTES` | 120 |

## No-Change Boundaries Confirmed

- `wire.rs` — `HookInput.transcript_path` already present, no edits
- `handle_compact_payload` — already on IndexBriefingService (crt-027), no edits
- `HookRequest::CompactPayload` wire format — unchanged
- `IndexEntry` / `format_index_table` — consumed as-is, not modified

## Open Questions for Other Agents

- **OQ-1** (not blocking): Injection-history affinity at compaction time. Pure fused score is acceptable for initial delivery; deferred to future feature.
- **SR-01 assumption**: SCOPE.md states "file is intact at PreCompact time" without evidence of flush ordering. Implementer should add a manual test step or note in the spec that verifies the file is flushed before the hook fires.
