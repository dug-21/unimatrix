# vnc-021-researcher Report

## Task
Revise SCOPE.md for vnc-021 to de-scope Prometheus metrics and change Claude Code to use curl-based shell hooks (same as Codex/Gemini).

## Changes Made

### Metrics De-scoping (8 edits)
1. Removed Goal #6 ("Add Prometheus-format metrics endpoint for production observability"); renumbered Goal #7 to #6
2. Removed `/metrics` intercept line from architecture diagram
3. Removed Key Design Choice #6 ("Metrics endpoint is unauthenticated")
4. Removed `/metrics (if enabled)` from AC-12 route list
5. Removed AC-14 (Prometheus metrics endpoint); renumbered AC-15 through AC-26 down by one (now AC-14 through AC-25)
6. Removed Open Question #1 ("Metrics scope"); renumbered remaining OQs
7. No `metrics_enabled` config references remained outside AC-14 (already removed)

### Curl-for-All-Clients (3 edits)
1. Updated AC-24 (was AC-25): changed from "Claude Code uses native HTTP hook handler" to "All three clients use curl-based shell hooks -- no native HTTP hook handler"
2. Updated "Resolved by ASS-064" section point 1: replaced Claude Code HTTP hook handler reference with curl-for-all-clients language
3. Verified no other `"type": "http"` hook handler references remain

### Final AC count: 25 (was 26)

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 15 results returned; relevant entries on vnc-001 security architecture, W2-3 StaticTokenAuth, W2-7 remote telemetry transport. No novel patterns discovered during scope revision.
- Stored: nothing novel to store -- scope revision only, no new patterns discovered

## Items for Human Review
1. ASS-064 FINDINGS.md line 164 still lists `/metrics` in the route example. This is the research document (what was explored), not the decision document. Left as-is since SCOPE.md captures the decision to exclude metrics. Flag if you want it updated for consistency.
2. AC numbering is now contiguous AC-01 through AC-25 with no gaps.
