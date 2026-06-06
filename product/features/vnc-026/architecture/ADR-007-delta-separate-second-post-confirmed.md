## ADR-007: Delta Carrier Confirmed — Separate Second POST, Sent Concurrently with the Carrying Event

### Context

RQ-5 resolved "separate second POST" with burden of proof on batching, and explicitly
allowed the architect to revisit. The alternatives: (a) piggyback the delta inside the
carrying event's payload, (b) batch both frames into one `RecordEvents` POST, (c) keep
two independent POSTs. AC-09 requires that a delta-send failure never fail or delay the
carrying event; the wire contract is frozen (no new envelope semantics); the per-event
Node process exits after the event loop drains, so "extra request" cost is wall-clock
inside an already fire-and-forget spawn, not host-visible latency.

### Decision

Keep the separate second POST (RQ-5 stands). Refinements:

- The two POSTs are issued **concurrently** (`Promise.allSettled`), not sequentially —
  the delta never delays the carrying event (AC-09 by construction), and the
  fire-and-forget spawn's wall-clock is max(POST1, POST2) instead of the sum.
- Outcomes are tracked independently: carrying-event failure → enqueue (ADR-003);
  delta failure → offset-non-advance (ADR-004). Neither outcome inspects the other.
- The delta POST is built only after a cheap `fstat` shows `file_len > last_offset`
  (no delta when the file is unchanged, AC-06) — most fire-and-forget spawns ship
  nothing extra.

Batching (b) remains rejected: `RecordEvents` would couple the failure domains (one 413
or timeout loses both frames), forces the 1 MiB guard to be shared between observation
payload and transcript span, and saves only one HTTP request on a path with no sync
budget. Piggybacking (a) is rejected outright — it would change the shape of existing
event payloads (frozen contract) and put transcript bytes inside frames that the queue
persists (violating ADR-004's at-rest guarantee).

### Consequences

- Easier: AC-09 independence is structural, not tested-for; ADR-003/ADR-004 failure
  handling stays orthogonal; frame-size math for SR-04 involves only the delta frame.
- Harder: two TCP/TLS handshakes per delta-carrying spawn (the per-event process cannot
  reuse connections) — irrelevant to the host CLI since the spawn is fire-and-forget,
  but it is the cost batching advocates would cite; revisit only if server connection
  pressure becomes measurable (F4's UDS path sidesteps it locally anyway).
