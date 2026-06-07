# Agent Report — vnc-026-agent-16-layer2

## Scope
Layer 2 integration suites (node:test) running the real client against the merged
F2 server (vnc-025, PR #692 — C-08 satisfied). AC-05 (two-layer parity, Layer 2
half + Layer 1 PreCompact byte-identity), AC-06, AC-07 (four pinned ADR-008
items), AC-10 concurrency.

## Files Created
- packages/unimatrix/test/helpers/real-server.js — real-server harness: spawns
  cargo-built `unimatrix serve --foreground` with HTTP /observe under a temp HOME;
  reserves a free port; writes config.toml; polls token + readiness; exposes
  `post()`, `precompact()`, and the ONE SR-11 `prepopulateBuffer()` helper.
- packages/unimatrix/test/helpers/layer2-fixtures.js — shared Layer 2 fixture
  helpers (exchange/JSONL shape, live/dead config, spawnDelta, register).
- packages/unimatrix/test/hook-client/parity-layer2.test.js — AC-05 (drops),
  AC-06 (grow/hold offset values), AC-07 (elision-mid-session, four pinned items).
- packages/unimatrix/test/hook-client/parity-layer2-concurrency.test.js — AC-10
  (>=8 interleaved sessions + injected drops, per-session byte isolation; raw
  session_id on wire / server mints http-).
- packages/unimatrix/test/hook-client/parity-layer2-precompact.test.js — AC-05
  Layer 1 PreCompact stdout byte-identity (spawned real client) using SR-11.

## Tests (pass/fail per AC)
- AC-05 (Layer 2 drops content-equivalence): 1/1 pass
- AC-05 (Layer 1 PreCompact byte-identity): 2/2 pass
- AC-06 (grow/hold offset values): 2/2 pass
- AC-07 (elision-mid-session, four pinned ADR-008): 1/1 pass
- AC-10 (concurrency byte isolation + raw-id wire): 2/2 pass
- Total my suites: 8/8 pass.
- Full package suite: 537 pass, 1 fail (known pre-existing
  `test_creates_mcp_json_on_clean_project`, out of scope), 1 skip (Windows-only
  `test_root_walk_windows_separators`, pre-existing). No new regressions.

## Key Design Decision (C-07 honored)
The four pinned ADR-008 items reference PRIVATE, #[cfg(test)]-only TranscriptBuffer
fields (holes/high_water/base_offset/elided_bytes) in session_transcript.rs that
are NOT reachable over the wire; C-07 forbids adding a query hook. The only
content-bearing wire surface is the server PreCompact restoration block built from
`TranscriptBuffer::contiguous_tail` (listener.rs handle_compact_payload →
BriefingContent → text/plain). The suites therefore assert the OBSERVABLE
CONSEQUENCES of each pinned item in that block (offset advance to file_len; no
post-elision starvation; tail tag present + seam crossed by post-elision delta;
no NUL bytes), which is the legitimate query surface the brief mandates.

## Issues / Blockers
- Concurrent-index hazard: a sibling agent's `git add` landed in the shared index
  between my `git add` and `git commit`, so the first commit captured the wrong
  files. Recovered with `git reset --soft` (no data lost) and re-committed using
  `git commit -o <my paths>` to commit ONLY my files atomically. No other agent's
  work was lost (their files remain untracked for them to commit).
- ass-071 SubagentStop stdin freebie: not captured — the Layer 2 suites drive the
  delta/PreCompact paths, not a SubagentStop event; out of this agent's path.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search/context_get —
  surfaced #4768 (node:test HTTP stub traps), #4774 (async spawn never spawnSync;
  state-dir derivation), #4758 (ADR-008 end-anchored + four pinned items),
  #4740 (vnc-025 buffer ADR-002). All applied.
- Stored: entry #4776 "Assert ADR-008 Layer-2 buffer state via the wire PreCompact
  block, not struct fields (C-07)" via /uni-store-pattern (topic unimatrix-package)
  — captures the wire-observable mapping + the real-server harness shape.
