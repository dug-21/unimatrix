# Agent Report — vnc-025-agent-1-pseudocode (Session 2 Stage 3a)

## Deliverables

| File | Component |
|------|-----------|
| pseudocode/OVERVIEW.md | Component interaction, data flow, shared types, lock discipline, build order, open questions |
| pseudocode/transcript-buffer.md | `TranscriptBuffer` state machine, `TranscriptPurgeRecord`, `session_key()` seam |
| pseudocode/transcript-block.md | Shared extraction core move + `extract_transcript_block_from_bytes` |
| pseudocode/registry-wiring.md | `SessionState.transcript`, ctors, `apply_transcript_delta`, `clear_transcripts_for_feature`, drain/sweep signature changes, poison-recovery helper |
| pseudocode/dispatch-wiring.md | Single-arm merge, batch tee, PreCompact tail-block build + prepend, drain/sweep call sites |
| pseudocode/purge-audit.md | Pinned audit event shape, `emit_purge_audits` helper, three-point wiring, #4379-safe emission context |
| pseudocode/config-knob.md | `transcript_buffer_max_bytes` field, validate floor, project-wins merge arm, ctor wiring |
| pseudocode/cycle-review-purge.md | tools.rs handler gate (exhaustive `TranscriptRetention` match), `retention_config` field plumbing |

All interface names traced to ARCHITECTURE.md Integration Surface or verified against the
codebase (line refs checked against main 2026-06-05/06). Wire surface consume-only
(`unimatrix-engine/src/wire.rs:46/:284`).

## Open Questions (full text in pseudocode/OVERVIEW.md)

1. `register_session` overwrite on `cycle_start` silently wipes a live session's transcript
   (unaudited purge) — architecture unaddressed; pseudocode keeps simple overwrite, flagged
   for SM/Gate 3a.
2. Purge pinned to *successful* review completion only (error paths keep transcripts) —
   confirm intent.
3. `server.rs:335` is the test-server ctor; brief lists it as a `with_transcript_cap` switch
   site — verify in review.
4. New wiring requirement not in the architecture file list: `retention_config:
   Arc<RetentionConfig>` field on `UnimatrixServer` (store_config #561 precedent) — the
   cycle-review handler has no retention access today.
5. Pseudocode-level pins within ADR latitude (clear() → `base_offset = high_water`;
   `bytes_purged = len()`; elision accounting rules; len-0 delta semantics;
   `contiguous_tail(0)` → None) — tester should pin via tests; crt-052 inherits.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned the feature's own ADRs (#4739-#4746,
  already read as files), vnc-024 #4720/#4721 (drop guard + retention enum, both incorporated),
  and pattern #4365 (exhaustive enum matching — reinforced the FR-16 exhaustive-match pin).
  No new constraints beyond the source documents.
- Deviations from established patterns: none. Audit emission follows the #4379/#302
  `log_event_async` + `tokio::spawn` pattern; config field follows the #561 store_config
  plumbing pattern; registry mutex keeps its existing `unwrap_or_else(into_inner)` idiom while
  the new buffer mutex gets the stricter ADR-008 clear-on-poison recovery.
