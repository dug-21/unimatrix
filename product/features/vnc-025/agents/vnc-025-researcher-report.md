# Agent Report — vnc-025-researcher

## Deliverable
- `product/features/vnc-025/SCOPE.md` — written, all required sections, AC-01..AC-15.

## Key Findings
- F1 (vnc-024) verified shipped in code: `TRANSCRIPT_DELTA_EVENT` + `TranscriptDeltaPayload` (wire.rs:46, :284), accept-and-drop guard in single-event arm (listener.rs:765-786) and batch filter (listener.rs:999-1009), `TranscriptRetention` enum with OSS `RetainDays` rejection + project-wins merge (config.rs:1506, :1634, :3376).
- Biggest structural constraint discovered: `SessionState: Clone` + hot-path `get_state()` deep-clone (session.rs:223; tools.rs:747, :1404) — transcript bytes must not ride wholesale clones (stored as pattern #4737).
- Cycle review has no feature→sessions registry index; distillation needs a new snapshot-and-clear registry method.
- PreCompact transcript block is currently built client-side only (hook.rs:246-255); empty-buffer condition naturally prevents double-prepend if the server-side block lands.
- Batch-arm non-persistence filter is load-bearing and must survive the buffer wiring (vnc-024 ADR-004 R-04).
- Issue #670 comment carries the F-07 `transcript_excerpt` size-cap obligation — scoped OUT (excerpt stays legacy/ignored).

## Open Questions for Human
Seven, in SCOPE.md §Open Questions — most consequential: distillation v1 depth (rule-based vs surface-to-calling-agent), PreCompact block in-scope confirmation, buffer bound default/policy, reconstruction-as-distillation-input-only narrowing.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #4721 (vnc-024 ADR-005 transcript_retention scope/secrets posture) and #4725 (transport-convergence guard testing), both load-bearing for scope; plus SessionState field-shape precedents (#3922, #759).
- Stored: entry #4737 "Do not add large payload fields directly to SessionState — get_state() deep-clones the whole struct on hot paths" via /uni-store-pattern.
