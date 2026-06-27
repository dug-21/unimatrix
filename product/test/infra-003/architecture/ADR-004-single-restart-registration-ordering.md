## ADR-004: Single-restart two-slug registration; route-liveness precondition, bidirectional content-read verdict

### Context

SCOPE Q5 asks whether to register both slugs before a single restart or to use a
second restart — both work because routing config is read once at boot. SR-11 is
the sharp risk: routing intent (`[[projects]]`) is applied **only at boot**
(#5079); `project register <slug>` creates a store but **not** a live route. If B
is registered *after* the restart, B's `ProjectEntry`/route never builds, so the
negative control would read a **non-existent** B store — and a "B has no marker"
read on a store that does not exist is a **false-pass**.

The `MultiProjectRouter` is built once at boot from the validated `[[projects]]`
slugs (`project_resolver.rs:172`); there is no runtime mutation. Two live routes
therefore require **both** slugs present in config before the boot that builds the
resolver.

Critically (SCOPE update), **route-liveness is not the isolation verdict**. A
non-404 response proves a route *exists*, but a **mis-resolved** route still
responds and runs the handler — non-404 ≠ isolated. The behavioral isolation
verdict is the **bidirectional 2×2 content read** (ADR-002), which is the only
thing that catches a B route mis-resolving into A's store.

### Decision

Register **both** A = `arch-research` and B = `isolation-b` **before the single
restart** (Q5 → the simpler single-restart flow), then run a **route-liveness
precondition** before the writes, and the **content-read verdict** after them:

1. Boot the shipped image; wait for `HTTP transport active`.
2. `project register arch-research` and `project register isolation-b` — both
   before the restart.
3. One `docker restart`; wait for `HTTP transport active` again.
4. **Route-liveness PRECONDITION (not a verdict):** assert all four routes
   (`/v1/A/observe`, `/v1/B/observe`, `/v1/A/mcp`, `/v1/B/mcp`) respond non-404
   *before* the C3/C4 writes. This exists to fail loud when a slug never built a
   route at all (the unregistered-B trap). It does **not** certify routing
   correctness — non-404 ≠ isolated.
5. **Write/read ordering and durability (owned by ADR-002, stated here for the
   flow):** the four marked writes are issued **strictly sequentially per store**,
   and each positive control is a **marker-keyed read-as-barrier**
   (retry-until-present, timeout → INFRA). This ADR's `store_size` usage is for the
   boot/liveness waits **only** — `store_size` is explicitly **not** the durability
   barrier (an aggregate "store grew" delta is unsound when a store takes two
   writes; see ADR-002). A read before the barrier is INFRA, never a verdict (AC-10).
6. **Behavioral verdict:** the bidirectional 2×2 two-store content read (ADR-002/
   ADR-003) — each store holds only its own slug's marker, both directions. This is
   what proves isolation, including B's route resolving to B's store.
7. Slug literals: A reuses the existing `arch-research`; B is the literal
   `isolation-b` — a neutral, test-scoped name (R-11) chosen so it cannot collide
   with a real-project store on the test volume (a name like `eval-baseline` would
   read like a live slug and risk contamination). The ADR-004 allowlist regex
   `^[a-z0-9][a-z0-9-]{0,62}$` is **authoritative and never re-typed** into the
   harness (SR-08/AC-13).

The single-restart flow is chosen over a second restart purely for harness
simplicity; the load-bearing requirement is "both in config before the boot that
builds the resolver," which one restart after two registrations satisfies.

### Consequences

- **Easier:** the negative control can only ever read a **real** B store, so a
  B-never-registered condition becomes an explicit RED at the liveness precondition,
  not a vacuous GREEN at the read (one half of SR-11). The other half — a B route
  that exists but mis-resolves into A — is caught by the bidirectional content-read
  verdict, not liveness.
- **Easier:** one restart is the simplest faithful flow and reuses the
  posture-smoke boot/register/restart idiom directly.
- **Harder:** the gate waits for `HTTP transport active` twice and probes four
  routes before writing — slightly more orchestration than the single-slug smoke;
  accepted, and bounded by the existing deadline-poll idiom.
- The route-liveness probe is a non-404 check only and must **not** write a marker
  (it would pollute the very stores the verdict reads); the four marked writes come
  after.

Related: ADR-001 (the gate that runs this), ADR-002 (the four-marker 2×2 read this
makes possible), ADR-003 (the bidirectional MCP writes that need both routes live).
