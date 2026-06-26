## ADR-006 nan-022: PreCompact Restoration Parity Over the Hook /observe Route — Measurability Determination with a Clean Delivery-Time Host-Side Call-Out

### Context
AC-06 requires the restored compact-context payload to be identical across legs. PreCompact
restoration rides the hook `/observe` route (`wire.rs:171` `CompactContext`/PreCompact, HookRequest
#670), NOT the MCP bridge — a different HTTPS surface than `context_*` (ADR-005). SR-07 flags a
MEASURABILITY risk: the restored payload may have a HOST-SIDE (Claude-Code) component the test-only
harness cannot drive — the nan-021 constraint forbids a live-CC-driven integration; the workload is
driven by the harness/bridge, not a live CC host. OQ-3 disposition is FIXED by the human: PreCompact
stays IN scope; if it cannot be driven test-only, that is a CLEAN delivery-time CALL-OUT (a
documented measurability limitation), NEVER a silent drop of the dimension.

### Decision
Keep PreCompact IN the matrix, route it on the `/observe` surface, and make any host-side gap an
EXPLICIT, captured, evidence-table-visible call-out — never an omission.

(1) **Capture shape carries the gap explicitly.** The `precompact` bundle entry is
`{"restored_payload": {...}|null, "measurable": bool, "host_side_gap": str|null}`. When the harness
CAN drive the `CompactContext`/PreCompact `/observe` frames symmetrically from both legs,
`measurable=True` and `restored_payload` is compared by `PreCompactComparator` field-for-field
modulo a closed wall-clock/ordering exclusion set (AC-06). When a host-side (CC) component is
required that the test-only harness cannot drive, `measurable=False`, `host_side_gap` names exactly
what is undrivable, `restored_payload=null`.

(2) **Measurability is a DELIVERY-TIME determination, surfaced loud.** Whether `CompactContext` is
symmetrically capturable from both legs is decided at first live drive (OQ-B). If `measurable=False`
on either leg, the dimension's outcome is NOT a silent pass and NOT a `PARITY_FAIL`: it is recorded
as a documented measurability limitation in the per-dimension evidence table (with `host_side_gap`),
and the gate output names PreCompact as "in scope, parity not test-only-measurable: <gap>" — the OQ-3
fixed disposition. This is the one dimension whose outcome may legitimately be a measurability
call-out rather than a parity verdict; the design makes that explicit, not implicit.

HONEST FRAMING FOR THE FLIP SESSION: the realistic outcome is likely "measured-where-drivable +
documented host-side gap" rather than a full symmetric measurement — the test-only harness cannot
drive a live Claude-Code host, so part of the restoration path may be undrivable. That is a
LEGITIMATE documented-exception disposition (C0 #5304's done_when + the human-signed exception
escape valve), but it MUST be STATED PLAINLY to the flip session — `measurable=False`/`host_side_gap`
surfaced verbatim. PreCompact parity is NEVER to be rounded up to "fully measured" when a host-side
gap exists.

(3) **Symmetry where measurable.** If drivable, BOTH legs emit the SAME #5298-style frames for the
PreCompact path (ADR-005), and the comparator compares the restored payload modulo only a closed,
justified wall-clock/ordering set (nan-021 ADR-003 discipline; membership of the set is
first-live-run + product-disposed).

(4) **Does NOT broaden server behavior.** It measures the shipped PreCompact path; it invents no new
restoration behavior (Non-Goal).

### Consequences
Easier: PreCompact cannot be silently dropped — the capture shape forces an explicit
measurable/host-side-gap decision visible in the evidence table; if drivable, it gets the same
closed-set parity discipline as every other dimension; the gap, if any, is named at delivery for the
human, satisfying OQ-3 by construction. Harder: this is the highest measurability-uncertainty
dimension — the harness may only be able to drive PART of the PreCompact path (the `/observe` write)
without the host-side restoration trigger, forcing a `measurable=False` call-out that leaves AC-06
partially asserted (a known, documented limitation, not a defect); the closed exclusion set for the
restored payload is unknown until the first live drive and is product-disposed; a `measurable=False`
PreCompact does NOT silently drop from the C0-flip bar — per C0 #5304's done_when (all six block,
confirmed 2026-06-25) it requires a HUMAN-SIGNED DOCUMENTED EXCEPTION via `blocks_c0_proof`, never a
quiet exclusion.

Related: SR-07, OQ-3 (fixed disposition); AC-06. Rides the `/observe` surface (ADR-005). Carries the
nan-021 ADR-003 (#5293) closed-set + product-disposition discipline. Pairs with nan-022 ADR-002
(outcome classes) and ADR-001 (registry `blocks_c0_proof`).
