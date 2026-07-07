## ADR-004: Isolation Invariants Are Proven Bidirectionally Through the Public `/v1/{slug}/...` Interface at N≥2 — the Behavioral Suite Is the Primary Gate; White-Box Guards Complement It; Reuse the #800 Fixture

### Context

Goal 3 (human directive, first-class deliverable): the durable guardrail must be
**solution-independent behavioral tests** that assert observable behavior through the public
HTTPS interface, so any future rewire that breaks cloud isolation trips them. Three risks shape
the seam:

- **SR-06 (bidirectionality):** a one-directional probe (write A, assert A-present / B-empty)
  **false-GREENs** the symmetric failure — B's route mis-resolving *into* A's store leaves the
  victim's own store correctly empty and a mis-routed handler still returns non-404 (lesson
  #5348, missed by two reviews). Every invariant must drive **each** slug's write through its
  **own** route and assert present-in-own **and** absent-in-every-other, in **both** directions.
  A `debug_assert` for the un-probed direction is compiled out (zero release coverage) — not
  acceptable.
- **N=1 blindness (#5172/#4974):** N=1 cannot distinguish a real per-slug funnel from a
  global-handle bypass. The suite runs at **N≥2** against **assembled production wiring** (POST
  delta via `route_observe` → read via that slug's `McpAdapter`), not hand-passed handles —
  existing tests inject one registry directly, which is exactly why instance-split bugs are
  invisible to them.
- **SR-05 (false confidence):** an invariant not observable through the public interface, if
  silently dropped, leaves the suite all-green over an unproven property. Any field lacking a
  clean public surface gets a white-box guard as a **documented** AC-06 exception, and the suite
  **enumerates** its own behavioral vs white-box coverage so gaps are visible, not implied.

### Decision

**The behavioral suite is the primary acceptance gate (AC-01…AC-07).** Setup: two registered
slugs A and B on one cloud instance; each driven only through its own `/v1/{slug}/...` surface.
Each invariant has a fidelity (own-read) half and an isolation (no-cross-read) half; both must
hold, and isolation is asserted **bidirectionally** (A↛B and B↛A):

- **INV-T1 (AC-01, #930):** delta posted to `/v1/{A}/observe` under a cycle → `cycle_review` via
  A's MCP surface folds it (non-empty candidates / transcript bytes).
- **INV-T2 (AC-02):** A and B use the **identical** `{phase}-{NNN}` cycle name; `cycle_review`
  via B never folds/counts/distills A's transcript, and vice versa.
- **INV-T3 (AC-03):** pending-entries analysis at A's `cycle_review` is never observable via B,
  and vice versa.
- **INV-K1/K2 (AC-04):** knowledge written to A is retrievable by A's observe-path
  briefing/search/compact; A's observe-path reads never return B's entries, both directions
  (closes the vnc-038 read-side gap, P2).
- **INV-C1/C2 (AC-05):** A's declared config governs only A's observable behavior, bidirectional,
  for every in-scope config field with a public surface.

**Public observation surfaces for INV-C** (SCOPE OQ-3): `transcript_signal_class_names` →
`signal_class_counts` in `cycle_review`; `observation_registry` → observation categories via
status; `store_config` → store byte-limit behavior; `retention_config` → purge behavior. Any
in-scope config field lacking a clean public surface (candidate: `inference_config` briefing
blend, `retention_config` purge timing) is covered by the ADR-003 boot assertion + a wiring-pin
unit test, recorded as a **documented AC-06 exception**. The suite ships an explicit
**coverage list** stating, per invariant, whether it is proven behaviorally or white-box.

**White-box guards complement, never substitute (AC-08, confirmed OQ-4):** the ADR-003 boot
assertion + `Arc::ptr_eq` wiring-pin units are required alongside the behavioral suite. The boot
assertion forecloses the class at boot; the behavioral suite proves the observable property
implementation-agnostically. The behavioral suite itself contains **no** `Arc::ptr_eq` and no
field-overwrite assertions (AC-06) — it stands alone and is implementation-agnostic.

**Fixture reuse (SR-08).** The INV-C1/C2 config-parity proof **reuses the #800 (infra-001)
multi-slug HTTP fixture** — extend it (cumulative test-infra rule), do not fork. This is also
C6's single path to proven; config-parity must be proven once, not twice. Confirm the fixture
owner with the tester / #800 before building INV-C fixtures. The transcript/knowledge invariants
extend the existing `project_routing_integration.rs` fixtures and pattern #5172 (N=2 model-free
cross-slug isolation).

### Consequences

- **Easier:** any future technology or wiring change that breaks cloud per-slug isolation trips
  a behavioral test through the public interface — the durable guardrail the goal demands, not
  tied to today's mechanism.
- **Easier:** the bidirectional N≥2 shape structurally cannot false-GREEN a reverse mis-route
  (SR-06 closed) or a global-handle bypass (N=1 closed).
- **Harder:** the suite is heavier than a unit test — assembled wiring, two slugs, both
  directions per invariant; and it must respect the #878 build-memory guardrails (`--jobs 1`
  link discipline for the large server test binaries). Reusing #800 amortizes fixture cost.
- **Boundary made visible:** the coverage list turns any behavioral gap into a declared,
  reviewable exception rather than a silent hole (SR-05).

Related: pattern #5172 (N=2 model-free isolation), lesson #5348 / pattern #5347 (bidirectional
test shape), #4974 (N=1 bypass), #800 (fixture reuse, SR-08), ADR-001/002 (the wiring proven),
ADR-003 (the white-box complements).
