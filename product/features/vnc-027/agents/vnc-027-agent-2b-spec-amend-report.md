# Agent Report: vnc-027-agent-2b-spec-amend

## Task
Targeted amendment of FR-30 and AC-10 in SPECIFICATION.md to align with amended ADR-006 (age-prune is the effective offset-deletion mechanism; TaskCompleted is an unreachable zero-cost provision keyed by canonical event equality).

## Changes Made
File: `product/features/vnc-027/specification/SPECIFICATION.md` — two edits, nothing else touched.

**FR-30** (was: trigger "moves ... to `TaskCompleted` and/or the existing age-prune"):
> The per-turn Stop→SessionClose offset delete is removed; the 7-day age-prune (`state.pruneOffsets`, wired into `runFireAndForget`) is the effective deletion mechanism. Per ADR-006 (authoritative): `TaskCompleted` is registered nowhere (`HOOK_EVENTS`, `.claude/settings.json`), so its delete branch is unreachable — it is retained only as a zero-cost provision keyed by canonical event equality, never frame type (Stop and TaskCompleted share the SessionClose frame), pinned by a unit test. The change is the keying only — no delta-streaming redesign (SR-08).

**AC-10** criterion (was: "keyed to `TaskCompleted` and/or age-prune"):
> Offset-file delete no longer fires on per-turn Stop→SessionClose; the 7-day age-prune is the effective deletion mechanism per ADR-006 (`TaskCompleted` branch retained but unreachable, keyed by canonical event name — never frame type); no full re-stream from offset 0 every turn.

AC-10 verification method updated to include the ADR-006 pinning test (TaskCompleted deletes; Stop must NOT).

## Result
A spec-only reader can no longer implement TaskCompleted-primary keying; ADR-006 is cited as authoritative in both items. FR-31, AC-12, and the workflow reference to AC-10 remain consistent without changes.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- top hit was ADR-006 itself (entry 4810); no conflicting conventions found.
