# vnc-025-docs — Documentation Agent Report

## Feature
vnc-025 — Server-Side Session Transcript Buffer (#670)

## Sources Read
- `product/features/vnc-025/SCOPE.md`
- `product/features/vnc-025/specification/SPECIFICATION.md`
- `product/features/vnc-025/agents/vnc-025-agent-5-config-knob-report.md` (64 KiB floor confirmation)
- `README.md`

No source code read.

## README Sections Modified

1. **Configuration → Key Config Sections, `[retention]` block**
   - Added `transcript_buffer_max_bytes` knob: default 4194304 (4 MiB), floor 65536 (64 KiB, below-floor aborts startup), ring-tail overflow with dropped-byte metadata, per-session bound (aggregate = cap x concurrent sessions). Traces to FR-10/FR-11, resolved decision 1, NFR-04, config-knob agent report (floor).
   - Updated `transcript_retention` `PurgeOnCycleClose` comment: now consumed — purges the in-memory buffer at session close, staleness sweep, and cycle review (`context_cycle_review`). Traces to Goal 3, FR-12/FR-15/FR-16.

2. **Security Model → new "Transcript Handling" subsection** (before Audit Trail)
   - Transcripts are in-memory only — never disk, SQL, or logs; in-memory + purge IS the secrets guarantee (Constraint 1, NFR-01, #4721).
   - Three purge triggers; content-free `transcript_session_purged` audit event (session_id, byte count, timestamp) on non-empty purge (Goal 4, FR-12–FR-14).
   - Sole buffer reader: server-side PreCompact transcript-tail block (Goal 5, FR-17).
   - Crash loses in-flight transcript by design (NFR-05).

## Sections Deliberately Untouched
- MCP Tool Reference, Skills Reference, CLI Reference — no tool/skill/CLI surface changed.
- MCP Transport `/observe` description — `transcript_delta` wire type was never documented in README at vnc-024; feature ships dark (no client streams deltas until F3), so no client-facing streaming docs added (no aspirational language).
- Data Layout — no on-disk artifact added (buffer never touches disk).

## Commit
`26bb72eec9ac5c75f0b61af2444729d312cde326` — `docs: update README for vnc-025 (#670)` on `feature/vnc-025` (README.md only).

## Self-Check
All items pass: artifacts read first, edits trace to SCOPE/SPEC claims, no source code read, README.md only modified, `docs:` prefix, no aspirational language, terminology consistent, no table-count drift (no tables changed). Knowledge Stewardship: exempt.
