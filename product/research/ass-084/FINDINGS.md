# FINDINGS: Production per-slug observe route-path structure, and can the Python harness emulate two-route isolation?

**Spike**: ass-084
**Date**: 2026-06-26
**Approach**: investigation (code-anchored)
**Confidence**: directional, evidence-backed

---

## TL;DR

- **GO.** The infra-001 HTTPS leg CAN faithfully drive two registered slugs over two
  distinct `/v1/{slug}/observe` routes and probe each per-slug store independently — and it
  can do so **in the SAME shipped multi-slug container**, with **no production-code change**.
- The gap is **single-route by current WIRING, not by construction.** The posture smoke
  hardcodes one slug (`SLUG="arch-research"`) and its D6 probe is a one-sided *directory
  count*, not a two-store read. The shipped server already multiplexes N slugs in one
  process via `MultiProjectRouter`; observe is `POST /v1/{slug}/observe` resolved through the
  SAME per-request `resolve_store` funnel as MCP. Two `project register` calls + a restart =
  two real routes, two real stores, in one container.
- This **decisively refutes ass-081's framing for the HTTPS leg.** ass-081 conflated two
  different "single-route" facts: UDS is single-route *by an enforced architectural guard*
  (ADR-006), but HTTPS is single-route *only because the harness registers one slug*. The
  second HTTPS route needs **no second daemon** (unlike UDS) — the funnel is already there.
- The recommended change drives the **exact** production seam
  (`parse_project_key -> resolve_store -> dispatch_request`) the SCOPE demands — it is a
  genuine isolation measurement, not a `feature=`-hint read.

---

## Findings

### Q1: Document the production per-slug observe route-path structure end to end — registration -> HTTPS observe route -> identity -> per-slug store resolution via dispatch_request — and what "two routes in production" concretely looks like.

**Answer:** The production path is a four-stage, fully per-slug funnel. There is **no
top-level `/observe`** and **no default store** any more (both deleted in vnc-038 ADR-003/004).

**1. Registration (creates the store + the routing intent).**
`unimatrix register <slug>` (server CLI) validates the slug against the allowlist
`^[a-z0-9][a-z0-9-]{0,62}$` (`seam.rs:83-116`, `ProjectSlug::try_from`), creates
`/data/.unimatrix/<slug>/` with the slug's OWN DB / vector index / hash-chain / analytics
(ADR-004 #4951), and adds the slug to `[[projects]]`. Routing intent is read **once at boot**
— a fresh registration requires a restart before it routes (the posture smoke does exactly
this: `docker ... project register arch-research` then `docker restart`,
`docker-http-posture-smoke.sh:420-423`). `register` is the **sole** store creator; a missing
store fails the daemon loud, never auto-creates (`main.rs:1121`).

**2. HTTPS observe route (the funnel edge).**
`PathRouter::call` (`router.rs:205-224`) splits exactly three ways: `GET /health`
(store-independent), then `POST /v1/.../observe` (matched by
`p.starts_with("/v1/") && p.ends_with("/observe")`, `router.rs:211`) -> `route_observe`;
everything else -> MCP via `SlugRouter`. The top-level `/observe` arm and the `DefaultResolver`
are **deleted** (vnc-038 ADR-003 #5082; `router.rs:200-204`, `273-277`).

**3. Identity (transport-derived, payload-immune).**
`route_observe` (`handlers.rs:40-65`) calls `seam::parse_project_key(path)`. The grammar is a
single rule: the slug is always **path segment 2** after `v1`, for BOTH
`/v1/{slug}/tools/...` (MCP) and `/v1/{slug}/observe` (`seam.rs:199-215`,
`test_parse_v1_slug_observe_returns_slug`). It yields `ProjectKey::Slug(slug)` — a type
**constructible only from the transport path, never from a request payload** (`seam.rs:33-57`),
so a client has no field with which to name another project. A no-slug / unregistered /
invalid path is a loud `RouteError` (404/400), **never** a default store (`seam.rs:158-163`).

**4. Per-slug store resolution -> dispatch (the shared seam).**
`route_observe` resolves the per-request store via
`observe_ctx.resolver.resolve_store(&ProjectKey::Slug(slug))` (`handlers.rs:66-80`). The
resolver is `MultiProjectRouter` — the **SAME** `Arc<dyn StoreResolver>` the `SlugRouter`
holds for MCP (`router.rs:80-102` `ObserveContext.resolver`; ADR-003 "one funnel, two entry
handlers"). `resolve_store` is a pure map lookup: `Slug(s) -> entry.store`, or
`UnknownProject` for an unregistered slug — it **never** falls back to a default or another
slug (`project_resolver.rs:204-211`). The resolved per-slug `Arc<Store>` is then threaded into
the transport-agnostic `dispatch_request` (knowledge #4691) as **both** the `store` and
`entry_store` params (`handlers.rs:150-163`); `dispatch_request` records the absorbed observe
data into THAT store. The boot-bound observe store was deleted (the #4974 ceremonial-funnel
guard) — there is no parallel observe path.

**What "two routes in production" concretely looks like:**

| | Slug A | Slug B |
|---|---|---|
| Registration | `register a` -> `/data/.unimatrix/a/` (own DB/vector/hashchain) + `[[projects]]` | `register b` -> `/data/.unimatrix/b/` + `[[projects]]` |
| Route path | `POST /v1/a/observe` | `POST /v1/b/observe` |
| Identity | `parse_project_key` -> `ProjectKey::Slug("a")` | `-> ProjectKey::Slug("b")` |
| Resolver entry | `MultiProjectRouter.slugs["a"] -> ProjectEntry{store_a, adapter_a}` | `slugs["b"] -> ProjectEntry{store_b, adapter_b}` |
| Lands in | `store_a` only | `store_b` only — `resolve_store(Slug("a"))` can never return `store_b` |

Both routes live in **one process / one container**: `MultiProjectRouter::from_servers` builds
one `ProjectEntry` per validated `[[projects]]` slug (`main.rs:1117-1235`,
`project_resolver.rs:172-195`), each over its own store + subsystems. Isolation is structural
— two distinct `HashMap` entries, no shared default — and is already unit-proven at the
resolver layer (`project_resolver/tests.rs`: `test_two_slugs_route_to_distinct_stores`,
`test_resolves_slug_to_its_store` — "alpha must never resolve to beta's store").

**Recommendation:** Treat the production observe surface as a **single per-slug funnel** — the
exact same `parse_project_key -> resolve_store -> dispatch_request` seam MCP uses. Any faithful
isolation test must drive `POST /v1/{slug}/observe` per slug and read back each slug's own
on-disk store; that is the production topology, byte for byte.

---

### Q2: Go/no-go — can the infra-001 Python harness FAITHFULLY drive two registered slugs over two distinct HTTPS observe routes and probe each per-slug store independently? Name the precise gap (single-route by construction vs. by current wiring).

**Answer: GO.** The HTTPS leg can faithfully drive two routes against the shipped multi-slug
container with **zero production change**. The gap is **entirely current wiring**, not
construction.

**The precise gap (current wiring, two parts):**

1. **One slug registered.** The posture smoke hardcodes `SLUG="arch-research"` and registers
   only it (`docker-http-posture-smoke.sh:27,420-421`). It already performs the full
   `register -> restart -> POST /v1/{slug}/observe -> assert per-slug store grew, hash dir
   unchanged` chain (Gates 1-4, lines 419-478) — but for exactly one slug.

2. **The D6 probe is a one-sided directory count, not a two-store read.**
   `capture_isolation_probe` (`cloud-bundle-lib.sh:107-140`) writes to A, then *counts other
   per-slug dirs* (`other_count`, line 127). With one slug `other_count` is structurally
   always 0 -> `visible_to_b=false` **trivially** (line 130-131). Worse, the code is built so
   that if a second slug dir merely *exists*, it flips to `visible_to_b=true` (lines 133-136)
   — it cannot actually read whether A's write is in B. So the current probe is *incapable* of
   a genuine two-slug cross read even if a second slug were present; it is a confinement
   *count*, not an isolation *measurement*. This is the #845 / ass-081 finding, confirmed.

**Why this is WIRING, not construction (Hypothesis-constraint #1 — challenged and refuted for HTTPS):**

ass-081 asserted the fixture is "single-store by construction ... by an enforced architectural
invariant (ADR-006)." That is true **only for the UDS leg**. ass-081 conflated two different
facts under one label:

- **UDS: single-route by construction.** The local boot path opens the path-hash store
  directly and is *forbidden* from entering the resolver by the ADR-006 compile-time guard
  (`local_binding_guard_tests.rs:36-48` `FORBIDDEN_IN_LOCAL` includes `MultiProjectRouter`,
  `parse_project_key`, `StoreResolver`, `ProjectKey`). A second UDS store needs a *second
  daemon* (ass-081 A1). No funnel is exercised.
- **HTTPS: single-route by current wiring only.** The shipped container's `MultiProjectRouter`
  **already** multiplexes N slugs in one process (`main.rs:1117-1235`, the per-slug loop).
  `register` is a supported operation the harness **already invokes**. Adding a second route
  is `register slug-b` + one more POST — it changes test *inputs*, not any server code path.

**Hypothesis-constraint #2 — challenged and confirmed:** "two faithful HTTPS observe routes
are reachable given the shipped multi-slug container." **Reachable.** The container hosts N
registered slugs by design; two registrations produce two `ProjectEntry` values and two live
routes `/v1/a/observe`, `/v1/b/observe` resolving to two distinct stores. The open question
ass-081 left ("driving and probing two of them") is answered: both the drive (a second
cert-pinned bearer POST, identical to the one Gate 2 already issues) and the probe (read each
slug's own `/data/.unimatrix/<slug>/unimatrix.db` via the existing `vol` busybox sidecar) use
machinery the harness already has.

**Charter compliance (Hard constraints):**
- *Test-only:* YES — no server / store-routing change; the server already routes two slugs.
- *Same route topology production uses:* YES — drives the real `POST /v1/{slug}/observe`
  funnel (`parse_project_key -> resolve_store -> dispatch_request`), not a `feature=` hint.
- *Extend infra-001 cumulatively:* YES — a second slug + a second POST + a genuine two-store
  read inside the existing posture smoke / `capture_isolation_probe`; no fork, no scaffold.

**Recommendation:** Build the two-route HTTPS observe isolation test (the N3 #5161 behavioral
test this spike was to unblock). It is feasible test-only and is the *stronger* of the two
legs — it exercises the actual per-request funnel where a mis-route could occur, which the
UDS second-daemon approach (ass-081 A1) structurally cannot.

---

### Q3: If go — rank the smallest faithful fixture/setup changes within the charter.

**Answer: GO; ranked below. Recommended = H1, a second slug registered in the posture smoke
with a genuine two-store read.**

**H1 — Second slug in the posture smoke + genuine two-store read. RECOMMENDED.**
- *Setup:* after the existing `register arch-research` + restart, also
  `docker run ... project register <slug-b>` and restart once more (or register both before a
  single restart). Both land as `[[projects]]` entries -> both build `ProjectEntry`s at boot
  (`main.rs:1124` loops over all `project_slugs`).
- *Drive:* POST a uniquely-marked observe event to `POST /v1/arch-research/observe` (the
  existing Gate-2 cert-pinned bearer POST, parameterized by slug). Optionally also POST a
  distinct marker to `/v1/<slug-b>/observe` for symmetry.
- *Probe (replaces the one-sided count):* via the `vol` sidecar, assert (a) A's marker write
  GREW `/data/.unimatrix/arch-research/` (already Gate 4), and (b) `/data/.unimatrix/<slug-b>/`
  did **not** gain A's write — `landed_only_in_a=true`, `slug_a_writes_visible_to_b=false`
  measured against a **real** second store, not `other_count`. For a true cross-read, read
  B's `unimatrix.db` (size/row delta) after A's write and confirm no growth attributable to A.
- *Charter:* test-only YES; same production funnel YES; cumulative on infra-001 YES (edits
  `docker-http-posture-smoke.sh` SLUG handling + `capture_isolation_probe` in
  `cloud-bundle-lib.sh`). **Smallest change that yields a genuine, non-vacuous D6 measurement.**

**H2 — Drive both routes through the bridge/observe path in the pytest orchestrator. Defer.**
- Extend the dual-leg orchestrator (`parity_legs_capture.py` / `test_https_uds_parity.py`) so
  the HTTPS leg captures a per-slug bundle for both slugs and a comparator checks the
  cross-read symmetrically. Faithful and aligns with nan-022's parity-matrix shape, but it is
  more surface than the minimal D6 fix and overlaps the "full fixture implementation" the
  SCOPE marks out of scope. Do this only if true probe-for-probe parity with the UDS leg is
  wanted (see Out-of-Scope #1).

**H3 — On-disk dir count only, second slug added. REJECT (still vacuous).** Merely adding a
second slug under the *current* `capture_isolation_probe` makes it report `visible_to_b=true`
(lines 133-136) because the count-based probe cannot read B — it would turn the gate falsely
RED. The probe semantics MUST change to a real two-store read (that is H1's core), not just
the slug count.

**Recommendation:** Adopt **H1**. It is the minimal faithful change: two real registrations,
two real `/v1/{slug}/observe` routes, and an on-disk two-store read that measures the N3
isolation property through the exact production seam.

---

### Q4: If no-go — name the blocker, the alternative, whether a documented exception is warranted, and what the ADR-006 UDS single-route guard proves on the local leg.

**Answer: Not no-go — the disposition is GO (Q2/Q3), so no documented HTTPS exception is
warranted.** Per the SCOPE's explicit ask, the UDS-leg contribution is recorded here:

- **No HTTPS D6 exception.** The isolation security property is faithfully measurable over
  HTTPS test-only (Q2). A "measured-where-drivable + documented gap" carve-out for the HTTPS
  observe leg is **not** justified and the #845 "directly analogous to the D5 PreCompact
  host-side gap" framing should be **withdrawn** for this leg — D5's gap is a live
  Claude-Code host no harness can drive; D6/HTTPS has no such host dependency.

- **What the ADR-006 guard proves on the LOCAL (UDS) leg.** The local STDIO/UDS path opens its
  path-hash store directly and is *structurally barred* from the resolver
  (`local_binding_guard_tests.rs`, G1-G4 + the meta-guard `test_guard_detects_local_routed_through_resolver`).
  This is a **non-silent, compile-time proof** that on the local transport there is **no
  second route to mis-route into** — cross-slug observe mis-routing is *unrepresentable*, not
  merely untested. So the local leg's isolation guarantee rests on the guard (a route cannot
  exist), while the HTTPS leg's guarantee must rest on a behavioral N=2 test (the route exists
  and must be shown to stay isolated). These are complementary, not a parity pair: there is no
  "two-route local analog" to be at parity with (the SCOPE's correct framing; C0 #5304 has no
  local two-route dimension here).

**Recommendation:** Keep #845 a `bug`, fix the HTTPS leg via H1 (Q3), and in any C0 flip-bar
language state plainly: *HTTPS observe isolation is measured behaviorally (N=2, two real
routes); local UDS observe isolation is guaranteed structurally by the ADR-006 compile-time
guard (no second route can exist).* Neither needs a D5-style host-side exception.

---

## Unanswered Questions

None of the four Goal questions are unanswered. Two items are explicitly left to the
implementing session (out of this directional spike's scope):

- **Exact form of the genuine two-store read in H1** — size/`du` delta vs. a row-count vs. a
  marker round-trip read of B's `unimatrix.db`. The SCOPE marks full fixture implementation
  out of scope. (The Gate-4 `store_size` `du`-delta idiom is the cheapest faithful primitive
  and is already in the smoke.)
- **Whether the second slug registers before a single restart or via a second restart** — a
  boot-timing/fixture detail; both work because routing is read once at boot.

No live two-slug Docker PoC was run: it was judged disproportionate for directional confidence
because every constituent mechanism is already independently proven — the shipped image's
single-slug behavioral chain (Gates 1-4: register -> restart -> `POST /v1/{slug}/observe` ->
per-slug store landing, hash dir unchanged) plus the resolver-layer N=2 isolation tests
(`test_two_slugs_route_to_distinct_stores`) — and the two-route case composes those mechanisms
with no new server code path. A Docker round-trip is the delivery's first-live-run gate, not a
prerequisite for the feasibility verdict.

---

## Out-of-Scope Discoveries

1. **Probe-for-probe parity between the legs is still not achieved even after H1.** The UDS
   leg's faithful probe (ass-081 A1) is a second-*daemon* cross-store wire read; the HTTPS leg
   measures a second-*slug* on-disk store read in one container. Both measure the same N3
   isolation property but through transport-native mechanisms (UDS has no funnel; HTTPS has
   one). For C0 framing this is correct (no local two-route analog), but anyone expecting the
   two legs to run byte-identical probes should know they legitimately differ. *Not pursued.*

2. **`capture_isolation_probe`'s "second store exists => leak candidate" heuristic
   (`cloud-bundle-lib.sh:133-136`) is a latent false-RED trap.** It was a defensible
   never-empty-pass guard while only one slug was ever registered, but it actively prevents a
   second slug from being added naively (H3). The implementing session MUST replace the
   count-based logic, not extend it. *Not pursued.*

3. **The same per-slug `resolve_store` funnel serves MCP and observe identically
   (`ObserveContext.resolver` is the SAME `Arc<dyn StoreResolver>` as `SlugRouter`'s).** A
   single N=2 isolation test could in principle cover both surfaces' routing in one shot, since
   mis-routing is structurally shared. Possible test-efficiency win for the C0 suite. *Not
   pursued.*

---

## Recommendations Summary

- **Q1 (route structure):** Production observe is a single per-slug funnel —
  `register <slug>` (creates `/data/.unimatrix/<slug>/` + `[[projects]]`) -> `POST
  /v1/{slug}/observe` -> `parse_project_key` => `ProjectKey::Slug` (transport-derived) ->
  `MultiProjectRouter.resolve_store` (same funnel as MCP) -> per-slug `Arc<Store>` ->
  `dispatch_request` writes there. "Two routes" = two registrations -> two `ProjectEntry`s ->
  two routes -> two isolated stores, **all in one container**.
- **Q2 (go/no-go):** **GO.** Faithfully emulable test-only. The gap is single-route by current
  WIRING (one hardcoded slug + a one-sided dir-count probe), **not** by construction — HTTPS,
  unlike UDS, already multiplexes N slugs in one process. ass-081's "by construction" claim
  holds only for the UDS leg.
- **Q3 (fixture change):** Adopt **H1** — register a second slug in the posture smoke, POST to
  its `/v1/{slug}/observe`, and replace the dir-count probe with a genuine per-slug two-store
  read. Defer H2 (orchestrator-level); reject H3 (still vacuous).
- **Q4 (exception?):** No HTTPS D6 documented exception — measurable test-only; withdraw the
  "analogous to D5" framing for this leg. The ADR-006 compile-time guard proves the LOCAL leg
  has no second route to mis-route into (structural guarantee), complementary to the HTTPS
  behavioral N=2 test.
