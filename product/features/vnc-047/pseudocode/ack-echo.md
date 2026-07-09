# C12 — Ack echo (best-effort, NON-GATING)

**File:** `crates/unimatrix-server/src/mcp/tools.rs` context_cycle handler `response_text` (~:4154-4168, goal-ack precedent)
**ADR:** ADR-007. **Risks:** R-16 (LOW, non-gating). **AC:** AC-09 (best-effort). **FR-12.**

## Purpose

On the EXISTING `context_cycle` ack string (no new interface, no read-back API), echo tag intake:
Start-with-tags → "accepted for recording" (explicitly NOT a durability guarantee); non-start with
tags → "ignored — only recorded at cycle start". Reuses the exact goal-ack fire-and-forget stance.
**Best-effort: MUST NOT block a gate.** Frozen-skip is NOT echoed (not caller-returnable).

## Inputs already in scope at :4154

- `validated.cycle_type` (`Start | PhaseEnd | Stop`) — already computed.
- `params.tags: Option<Vec<String>>` (C6) — the caller's own input. Does NOT read `phase`, does NOT
  read stored `cycle_tags` (respects Non-Goal #6 — echoes input, never the freeze outcome).

## Pseudocode — extend the `response_text` construction (:4154-4168)

```
# Compute a best-effort, non-empty-filtered view of the caller's tags (same non-empty filter
# as the write path; no other validation — value-opaque). Empty/all-blank => no phrase.
let tag_view: Vec<&str> = params.tags.as_deref().unwrap_or(&[])
    .iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();

# existing base ack (goal arm / no-goal arm) stays as-is; then append a tag phrase:
let tag_phrase: String =
    if tag_view.is_empty() {
        String::new()                              # no tags supplied → unchanged ack
    } else if validated.cycle_type == CycleType::Start {
        format!(" {} run-identity label(s) accepted at cycle start: [{}]. \
                 Recording is fire-and-forget; use context_cycle_review to confirm.",
                tag_view.len(), tag_view.join(", "))
    } else {
        " tags ignored — labels are only recorded at cycle start.".to_string()
    };

let response_text = format!("{}{}", base_ack, tag_phrase);
```

### Notes

- **Accept-for-recording, NOT durable-confirm** — wording parity with the existing goal ack ("…
  fire-and-forget … use context_cycle_review to confirm", :4157-4158). The ack cannot promise the
  freeze outcome (that's C13's trace + `cycle_review`).
- **Pure string addition, no business logic, no fallibility** — an empty/all-blank list yields no
  phrase (never an error). Cannot block the call.
- No `phase` read → the first-statement phase-snapshot discipline is not engaged (ADR-007).
- Exact strings are illustrative; testers assert the accept-for-recording note on start-with-tags and
  the "ignored" note on non-start-with-tags — a miss does NOT fail delivery (AC-09 best-effort).

## Error handling

None — additive text; degrades to no phrase.

## Key test scenarios (hints — NON-GATING, verify only)

1. Start with tags → ack contains an accept-for-recording note naming N and the labels.
2. Phase-end/stop with tags → ack contains the "tags ignored — only recorded at cycle start" note.
3. Start/any event with no tags → ack unchanged (no tag phrase).
> Do NOT block a gate on these (R-16/AC-09). Frozen-skip is intentionally NOT surfaced here.
