# Agent Report: col-025-docs

## Status: Complete

## Sections Modified

1. **MCP Tool Reference — `context_cycle` row** (line 329)
   - Added `goal` parameter documentation: optional, `start` only, 1–2 sentence plain-text statement of feature intent, max 1 024 bytes, used as step-2 query signal by `context_briefing` and hook injection when no explicit `task` is provided.
   - Traces to: SPECIFICATION.md FR-01, FR-03 (MAX_GOAL_BYTES = 1 024), SCOPE.md Goal 1.

2. **MCP Tool Reference — `context_briefing` row** (line 327)
   - Updated query derivation step 2: was "synthesized from session `feature_cycle` + top topic signals", now "active cycle `goal` when set (stored via `context_cycle(start, goal: ...)`)".
   - Added `CONTEXT_GET_INSTRUCTION` header note: response now starts with a header line before the indexed table.
   - Traces to: SPECIFICATION.md FR-06, FR-07, FR-12.

3. **Architecture Overview — Storage section** (line 423)
   - Updated schema version from 15 to 16.
   - Traces to: SPECIFICATION.md FR-10 (v15→v16 migration).

4. **Architecture Overview — Data Layout block** (line 452)
   - Updated `unimatrix.db` comment from `schema v15` to `schema v16`.
   - Traces to: SPECIFICATION.md FR-10.

## Commit

`0292e88` — `docs: update README for col-025 goal signal parameter (#374)`

## Fallback Chain

SPECIFICATION.md was present and used as the primary source. No fallback required.

## Sections Not Modified

All other README sections were unaffected by this feature. No sections were left undocumented due to insufficient artifact detail.
