# C12 — Ack echo (best-effort, NON-GATING)

> File: `crates/unimatrix-server/src/mcp/tools.rs` context_cycle handler `response_text` (~:4154-4160,
> goal-ack precedent). Best-effort tag phrase on the EXISTING ack string. No new interface.
> Risks: R-16 (Low). ACs: AC-09.
> **NON-GATING. Verify strings if implemented; a miss here MUST NOT block delivery/any gate.**

## Test expectations (best-effort, unit/handler-string level)
- `test_ack_start_with_tags_accept_for_recording` — a Start-with-tags call's ack `response_text`
  contains the accept-for-recording note (e.g. "N labels accepted for recording… use
  context_cycle_review to confirm"). Wording is accept-for-recording, explicitly NOT a durability
  guarantee. (Also verifiable via Python `test_context_cycle_ack_echoes_tags` — OVERVIEW §5.)
- `test_ack_non_start_with_tags_ignored_note` — a non-start-with-tags call's ack contains the "tags
  ignored — only recorded at cycle start" note.
- `test_ack_no_tags_unchanged` — a call with no tags produces the pre-vnc-047 ack unchanged (no
  spurious tag phrase).

## Constraints
- The ack echoes the CALLER's own input ("accepted for recording") — it MUST NOT read stored
  `cycle_tags` (Non-Goal #6; no read-back API).
- The frozen-skip outcome is NOT caller-returnable — do NOT assert a caller-visible frozen-skip
  signal here (that observation lives in freeze-trace.md as a listener log only).
- Reuses the existing ack string — assert NO new MCP interface/field is added.

## Gate posture
Explicitly NON-GATING (R-16/FR-12/AC-09). Record the result in RISK-COVERAGE-REPORT.md; do NOT fail
a gate on a miss. If deferred, note "AC-09 deferred, non-gating" — acceptable.
