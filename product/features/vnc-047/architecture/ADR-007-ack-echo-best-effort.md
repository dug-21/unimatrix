## ADR-007 vnc-047: Best-effort ack echo in the existing context_cycle response; frozen-skip surfaced as listener tracing only

### Context

Tag persistence is fire-and-forget on the UDS hook path (ADR-002/003): the MCP handler never learns
whether tags were stored, and there is no read-back API before `cycle_cycle_review`. Still, an operator
stamping run-identity labels benefits from immediate, best-effort feedback that the labels were
*accepted for recording*. This must be a nice-to-have SHOULD — it must not become a blocking
requirement, must add no new MCP interface, no read-back API, and must respect the fire-and-forget
write. Two candidate signals were assessed for clean fit:

(a) **MCP response echo** — the handler already builds a synchronous ack string from
`validated.cycle_type` and the request params (tools.rs:4128-4168; the `goal` arm at :4154-4160 is the
exact precedent). `params.tags` and `cycle_type` are both in hand at that point.

(b) **Frozen-skip vs wrote-set outcome** — whether the whole-set-once EXISTS guard inserted or skipped
(ADR-002 §3) is known **only** inside the listener transaction, on the UDS side. Returning it to the
caller would require a new interface — out of scope.

### Decision

Both parts fit cleanly; nothing is dropped.

**(a) Ack echo in the existing response string** (handler-synchronous, no new interface):

- On a **Start carrying tags**: append, to the existing ack, a phrase of the form
  `"N run-identity label(s) accepted at cycle start: [<labels>]. Recording is fire-and-forget; use
  context_cycle_review to confirm."` `N`/`[<labels>]` are the non-empty-filtered submitted tags
  (same non-empty filter as the write path; no other validation — value-opaque). This is
  **accept-for-recording, NOT durable-confirm** — identical stance to the existing goal ack
  (":4154-4160", which already says "fire-and-forget … use context_cycle_review to confirm").
- On a **non-start event carrying tags**: append a concise
  `"tags ignored — labels are only recorded at cycle start."` (parity with how goal is silently
  ignored on PhaseEnd/Stop, but made explicit for tags since they are a new affordance).
- On a start with no tags, or any event with no tags: no tag phrase (unchanged ack).

The handler needs only `validated.cycle_type` (already computed) and `params.tags`. It does **not** read
`phase` for this, so the first-statement phase-snapshot discipline is not engaged. The echo is built in
the same `response_text` construction that already branches on `validated_goal`; it is a pure string
addition with no business logic and no fallibility (an empty/all-blank tag list simply yields no
phrase). Because it is best-effort text, it can never block the call.

**(b) Frozen-skip is listener tracing only** (not in the MCP response):

- In `insert_cycle_start_with_tags` (or its caller in the step-5 spawn), emit a concise `tracing`
  record distinguishing the two outcomes: wrote-set (`cycle_tags: recorded N labels for feature_cycle`)
  vs frozen-skip (`cycle_tags: set already frozen for feature_cycle, N submitted labels ignored`).
- This is operator-visible via logs only. It is **not** returned to the caller — doing so needs a new
  interface (out of scope). The ack echo (a) deliberately does not promise the freeze outcome; it
  promises acceptance-for-recording and points to `cycle_review` for the authoritative set.

### Consequences

- Easier: the operator gets immediate, honest feedback on Start (accepted-for-recording) and on the
  no-op case (non-start ignore), reusing the exact goal-ack pattern — zero new surface.
- Easier: the whole-set-once freeze outcome is observable in logs for debugging A/B stamping without
  inflating the MCP contract.
- Accepted gap: the ack cannot tell the caller whether *this* start's labels won the one-shot or were
  frozen-skipped — that truth lives only in `cycle_review` (authoritative) and the listener trace. This
  is the deliberate fire-and-forget boundary (SR-03), not a defect.
- No blocking behavior: the echo is additive text; a malformed/empty tag list degrades to no phrase,
  never an error.
- Cross-ref ADR-002 (whole-set-once write + freeze), ADR-004 (cycle_review is the authoritative
  read-back the ack points to).
