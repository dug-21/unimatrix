# infra-003: Standalone Multi-Tenant HTTP Isolation Test

> Test-only feature. Extends the infra-001 HTTP integration harness cumulatively
> (no fork, no scaffold). Design artifacts live under `product/test/infra-003/`
> to match the infra-001 / infra-002 precedent. Advances capability **N3 (#5161 —
> writes never mis-routed across projects)**; does NOT by itself close it.
> GH issue: **#853**.

## Problem Statement

Cross-tenant isolation — "a write for slug A can only ever land in A's store" — is
the integrity basis of the personal-cloud model (`goal:personal-cloud`). A
mis-routed write corrupts the wrong project's hash chain unrollbackably and
silently; a *vacuous* test of this failure mode is worse than none.

This property is **HTTP-only**. It has no local-UDS analog: local STDIO/UDS is
1-client:1-project by the ADR-006 (#5087) compile-time guard, which opens the
path-hash store directly at boot and is structurally barred from the HTTP
resolver (`local_binding_guard_tests.rs::FORBIDDEN_IN_LOCAL`). There is no second
local route to mis-route into.

It was mistakenly scoped as a *parity* dimension (D6) inside the nan-022 / C0
cross-transport parity matrix, where it could only ever be vacuous or contorted
(there is no local two-route leg to be "at parity" with). D6 was removed from the
parity matrix in #845 (#5332). The genuine behavioral isolation test — the real
work surfaced by ass-084 (#850, GO) and the #845 investigation — belongs here,
standalone and single-surface, decoupled from parity.

The shipped container already multiplexes N slugs in one process via
`MultiProjectRouter` (vnc-038, #5082). This feature **exercises** that existing
production routing; it does not change it.

Slugs: **A = `arch-research`** (existing), **B = `isolation-b`** (neutral,
test-scoped literal — see Q4 / R-11).

The test is **bidirectional** across **two write surfaces** — a 2×2 matrix.
Distinctly-marked writes are driven through **both** slugs' routes (A and B) on
**both** surfaces (observe and MCP-write), and the two-store content read asserts
the full discrimination per surface: **A's store holds only A's marker (not B's),
B's store holds only B's marker (not A's)**. Single-direction (write only through
A) cannot catch the symmetric N3 failure — B's route mis-resolving to A's store —
because B-on-disk would correctly read empty and the negative control would pass
GREEN on a broken B route. B's isolation earns the same behavioral proof as A's.

1. **Observe surface, both directions.** Drive a uniquely-marked observe write
   through `POST /v1/A/observe` **and** a distinct marker through
   `POST /v1/B/observe`, each via the genuine production funnel
   `parse_project_key → resolve_store → dispatch_request`.
2. **Positive controls (load-bearing):** via a genuine content read (not a size
   delta), A's store contains A's observe marker **and** B's store contains B's
   observe marker.
3. **Negative controls (cross-contamination):** via the genuine two-store read
   (not a dir-count / `other_count` heuristic), A's store does **not** contain B's
   observe marker **and** B's store does **not** contain A's observe marker.
4. **MCP-write surface, both directions** (Q1 resolved → option (a)): repeat the
   full 2×2 with distinct markers via `POST /v1/A/mcp` and `POST /v1/B/mcp`
   (`context_store` / `context_correct` JSON-RPC). This is the **load-bearing
   half**: the store↔adapter equality the observe path relies on (`entry.store` ==
   the store inside `entry.adapter`) is guarded only by a `debug_assert!` compiled
   **out** of the release container, so MCP-write isolation has **zero behavioral
   coverage in the shipped artifact** without this probe.
5. Do all of this **test-only**, extending the infra-001 harness cumulatively in
   the shipped multi-slug container, with no production-code change.

## Non-Goals

- **Claiming N3 proven / closing N3 (#5161).** Even with both the observe and
  MCP-write surfaces covered, this is a **point-in-time** proof. N3 stays
  `partial` because the standing regression gate (N5, #788) is not wired here — a
  point-in-time pass does not guard against future regressions. infra-003 advances
  N3, it does not close it.
- **No UDS behavioral probe.** The ADR-006 compile-time guard
  (`FORBIDDEN_IN_LOCAL`) is *referenced as proof that a second local route is
  unrepresentable*, never re-run as a behavioral test.
- **No parity comparison and no entry in the parity harness.** D6 was removed
  (#845); this test must not reintroduce a parity shape. No probe-for-probe parity
  with the UDS leg.
- **No production routing change.** Slug routing, the resolver, and store binding
  are exercised as shipped, not modified.
- **No new pytest suite / orchestrator-level dual-leg capture** (ass-084 H2 is
  deferred). The minimal cumulative extension is the target, not a re-architecture.

## Background Research

### Production routing — the real funnel the test exercises
(`crates/unimatrix-server/src/http/router/`)

- There is **no** top-level `/observe` and **no** default store (both deleted in
  vnc-038 ADR-003/004, #5082). `PathRouter::call` splits three ways: `GET /health`
  (store-independent), `POST /v1/.../observe` → `route_observe`, everything else →
  MCP via `SlugRouter`.
- `parse_project_key` (`seam.rs:199-215`) takes the slug from **path segment 2**
  for both `/v1/{slug}/observe` and `/v1/{slug}/mcp`, validated at the parse edge
  against the allowlist; it yields `ProjectKey::Slug`, **constructible only from
  the transport path, never a payload** (no client-named project).
- `route_observe` (`handlers.rs`) calls `resolver.resolve_store(&ProjectKey::Slug)`
  **per request** (no boot-bound store; the #4974 ceremonial-funnel trap is
  closed). `MultiProjectRouter::resolve_store` (`project_resolver.rs:204`) is a
  pure `self.slugs.get(s) → entry.store` lookup, `UnknownProject` otherwise — never
  a default, never another slug. The resolved `Arc<Store>` is threaded into
  `dispatch_request`.
- "Two routes in production" = two `register` calls → two `ProjectEntry`s at boot
  → two live routes `/v1/A/observe`, `/v1/B/observe` → two isolated stores, **all
  in one container** (`main.rs` per-slug loop). Already unit-proven at the resolver
  layer (`test_two_slugs_route_to_distinct_stores`).

### Shared seam — observe vs MCP-write (code investigation for Q1)

Both surfaces share the **same** isolation seam: the same `parse_project_key`
function and the **same `Arc<dyn StoreResolver>` instance** (built once at boot,
`Arc::clone`d into both `ObserveContext.resolver` and `SlugRouter`), both invoked
**per request** with no boot-time store capture. The cross-tenant *routing
decision* (key parse + per-slug `HashMap` lookup keyed by `ProjectKey::Slug`) is
therefore structurally identical for observe and MCP-write.

**Nuance (the load-bearing reason this is an Open Question, not a settled Goal):**
after the shared key/lookup, dispatch diverges. Observe goes straight to
`dispatch_request(resolve_store(...))`. MCP-write goes through `adapter_for(key)`
→ a **per-slug `McpAdapter` that holds its own store captured at boot** (a sibling
field of the same `ProjectEntry{store, adapter}`). The observe N=2 test asserts
`entry.store` isolation; it does **not** exercise that `entry.adapter`'s writes
land in `entry.store`. In a correctly built `ProjectEntry` these are the same
`Arc<Store>`, but that equality is a construction invariant, not something the
observe test drives. So the observe test proves the *shared routing key/lookup* is
isolated — strong transitive evidence — but is **not** byte-identical coverage of
the MCP write surface.

### Existing harness machinery to extend (no scaffold)

- `scripts/docker-http-posture-smoke.sh` already does the full single-slug chain:
  `register <slug>` → `docker restart` → cert-pinned bearer `POST
  /v1/{slug}/observe` → `204` → assert per-slug store grew (`du -s` over the slug
  dir, WAL-robust) and hash store unchanged (Gates 1–4). Token + TLS cert are
  pulled from the path-hash dir via the `vol()` busybox sidecar (distroless image:
  **all** volume inspection is via the sidecar, never `docker exec`).
- `scripts/cloud-bundle-lib.sh` already has the content-read idiom: `vol cat` the
  per-slug `unimatrix.db` **plus its `-wal`/`-shm` sidecars** out to a sandbox,
  then `sqlite3 -json` query host-side, with a **hard INFRA fail if sqlite3 is
  absent** (provisioned like node) and after the durability barrier (#5321).
- The old D6 `capture_isolation_probe` (dir-count / `other_count`) was removed with
  D6 (#845); this feature builds the genuine two-store read fresh — it does not
  extend the count-based logic (ass-084 Out-of-Scope #2: that heuristic is a
  latent false-RED trap, must be replaced not extended).

### Carried lessons

- **SR-09 / #4975:** the slug allowlist constant drifts when re-typed. Both slugs
  must be valid under the ADR-004 allowlist `^[a-z0-9][a-z0-9-]{0,62}$`:
  A = `arch-research` (existing), B = `isolation-b`. The ADR is authoritative, not
  any restated regex.
- **R-11 (test-scoped literal):** B is `isolation-b`, not a real-project-sounding
  name. A literal like `eval-baseline` reads like a live eval-harness slug and
  could collide with a pre-existing store on the test volume, contaminating the
  two-store read; the neutral `isolation-b` is the structural fix.
- **#5079:** routing config is read **once at boot**; `register` creates a store
  but not routing intent. Both slugs must be registered before the (single)
  restart that applies `[[projects]]`.
- **#4950 invariant 1:** identity comes from the transport, never the payload. If B
  is read over the wire it must use **B's own per-slug credentials**; for a pure
  store-separation assertion an on-disk read of B via the `vol` sidecar is
  sufficient and simpler.
- **#5321 (marker-keyed read-as-barrier):** the own-store positive read is a
  bounded retry-until-the-marker-is-present (not an aggregate `du`/durability
  barrier); an own-store timeout is INFRA, never a verdict (RED). The cross-store
  negative read follows the confirmed own-store marker. WAL `-wal`/`-shm` sidecars
  must be copied with each main db or the read sees a false-empty pre-checkpoint
  snapshot.

## Proposed Approach

Add a standalone, single-surface HTTP isolation gate that reuses the shipped
multi-slug container and the existing posture-smoke machinery (ass-084 **H1**,
the recommended option):

1. Register **both** slugs (A = `arch-research`, B = `isolation-b`) before a single
   restart (both become `ProjectEntry`s at boot).
2. Drive **four distinctly-marked writes** over the existing cert-pinned bearer
   path (the slug is in the URL path, so one bearer token serves all four):
   - `POST /v1/A/observe` (marker `A-obs`) and `POST /v1/B/observe` (marker
     `B-obs`) — the real `parse_project_key → resolve_store → dispatch_request`
     funnel; expect `204`.
   - `POST /v1/A/mcp` (marker `A-mcp`) and `POST /v1/B/mcp` (marker `B-mcp`) —
     `context_store` / `context_correct` JSON-RPC; expect success.
3. Using the **marker-keyed read-as-barrier** (bounded retry-until-present on each
   own-store; timeout → INFRA, never RED), do a **genuine two-store read** (both
   stores) via the `vol` sidecar + `sqlite3` content-read idiom (WAL sidecars
   included) and assert the full 2×2 matrix per surface:
   - **Observe:** A's store contains `A-obs` and **not** `B-obs`; B's store
     contains `B-obs` and **not** `A-obs`.
   - **MCP-write:** A's store contains `A-mcp` and **not** `B-mcp`; B's store
     contains `B-mcp` and **not** `A-mcp`.
   Each surface's positive controls gate its negative controls: a silently-failed
   write (own marker absent from its own store) fails RED — it must never pass
   vacuously as "the other store is clean."

Rationale: H1 is the smallest faithful change — two real registrations, four real
routes (2 slugs × 2 surfaces), one on-disk two-store content read — measuring N3
bidirectionally through the exact production seam. The extra direction is ~2 extra
writes against the reads that already hit both stores, doubling the
discrimination. It reuses the cert/token/`vol`/sqlite3 primitives already in the
harness (no new transport, no new spawn path). H2 (pytest orchestrator dual-leg)
is more surface than the property needs and is deferred; H3 (dir-count + second
slug) is rejected as still vacuous.

## Acceptance Criteria

All four writes use **mutually non-substring** markers
`infra003-{obs,mcp}-{a,b}-<run>` (i.e. `infra003-obs-a-<run>`,
`infra003-obs-b-<run>`, `infra003-mcp-a-<run>`, `infra003-mcp-b-<run>`, with
`<run>` unique per run) so every cell of the 2×2 matrix is independently
attributable and no marker is a substring of another (a substring match for one
must never spuriously match another). Referred to below by the aliases `A-obs`,
`B-obs`, `A-mcp`, `B-mcp`.

The read is **marker-keyed read-as-barrier**: the own-store positive read is a
bounded retry-until-the-marker-is-present (replacing any aggregate `du`/durability
barrier); an own-store timeout (marker never appears) is **INFRA, never RED**. The
cross-store negative read is asserted only after the corresponding own-store marker
is confirmed present.

### Registration

- **AC-01:** Both slugs (A = `arch-research`, B = `isolation-b`) are registered
  **before a single restart**; after restart all four routes — `/v1/A/observe`,
  `/v1/B/observe`, `/v1/A/mcp`, `/v1/B/mcp` — are routed (registered slugs build
  `ProjectEntry`s at boot). Route-liveness alone is **not** a verdict (a
  mis-resolved route still responds non-404).

### Observe surface (bidirectional)

- **AC-02:** Marked observe writes are POSTed to `POST /v1/A/observe` (`A-obs`)
  **and** `POST /v1/B/observe` (`B-obs`) via the real cert-pinned bearer path, each
  returning `204` (the genuine `parse_project_key → resolve_store →
  dispatch_request` funnel).
- **AC-03 (positive controls, load-bearing):** A **content read** via the
  marker-keyed read-as-barrier (bounded retry-until-present) confirms A's store
  **contains `A-obs`** **and** B's store **contains `B-obs`** — presence assertions
  on the markers, not `du` size deltas. An own-store timeout is INFRA, never RED.
- **AC-04 (negative controls — cross-contamination, both directions):** A
  **genuine two-store read** confirms A's store does **not** contain `B-obs`
  **and** B's store does **not** contain `A-obs` — measured against each real
  store, never a dir-count / `other_count` heuristic. (The B-direction is what
  catches B's route mis-resolving into A's store.)
- **AC-05:** Each direction's positive control gates its negative control: if a
  slug's own observe marker is absent from its own store, the test fails RED and
  does not report the other store's cleanliness as a pass.

### MCP-write surface (bidirectional; same container / slugs / cert / `vol` / sqlite3)

- **AC-06:** Marked MCP writes are sent to `POST /v1/A/mcp` (`A-mcp`) **and**
  `POST /v1/B/mcp` (`B-mcp`) (`context_store` / `context_correct` JSON-RPC, tool
  name in the body) via the real cert-pinned bearer path, each returning a success
  response (same `parse_project_key → resolve_store` routing, then per-slug
  `adapter_for` dispatch).
- **AC-07 (positive controls, load-bearing):** A content read confirms A's store
  **contains `A-mcp`** **and** B's store **contains `B-mcp`**.
- **AC-08 (negative controls — cross-contamination, both directions):** A genuine
  two-store read confirms A's store does **not** contain `B-mcp` **and** B's store
  does **not** contain `A-mcp`.
- **AC-09:** Each direction's MCP positive control gates its negative control (no
  vacuous pass on a silently-failed MCP write).

### Shared discipline

- **AC-10:** The read is **marker-keyed read-as-barrier**: the own-store positive
  read is a bounded retry-until-the-marker-is-present (not a separate aggregate
  `du`/durability barrier); the cross-store negative read is asserted only after
  the own-store marker is confirmed present. An own-store read that times out
  (marker never appears) is an **INFRA error, never RED** — it is never reported as
  a verdict.
- **AC-11:** Content reads that require `sqlite3` **hard-fail INFRA** when
  `sqlite3` is absent (provisioned like node) — never a silent empty capture that
  empty-passes. WAL `-wal`/`-shm` sidecars are copied with each main db so the
  post-barrier durable view is read.
- **AC-12:** Each slug's store is read correctly: via that slug's **own per-slug
  credentials** (over the wire) or via an **on-disk read** through the `vol`
  sidecar (the Gate-4 idiom). One slug's credential is never reused to read
  another's store.
- **AC-13:** The test is **test-only and cumulative on infra-001** — no production
  code change, no fork/scaffold. Slug literals are not re-typed copies of the ADR
  allowlist value; B = `isolation-b` is a neutral test-scoped literal (R-11),
  chosen to avoid collision with any real-project store on the volume.
- **AC-14:** The local-UDS isolation guarantee is **referenced** (ADR-006
  compile-time guard / `FORBIDDEN_IN_LOCAL`), not re-run; no UDS behavioral probe
  is added and no parity-matrix shape is introduced.
- **AC-15 (MCP per-session isolation — correctness sub-property of Goal 4):** Each
  MCP-write probe (`POST /v1/A/mcp` and `POST /v1/B/mcp`) captures and uses its
  **own** `Mcp-Session-Id` over the streamable-HTTP handshake; A's session is never
  reused against B's route (a crossed session would mis-attribute the test).
  INFRA-vs-RED discrimination holds for both probes: a handshake/session failure is
  **INFRA**, while a marker landing in the wrong store is **RED**.

## Constraints

- **Test-only, cumulative:** extend the infra-001 harness; no production change, no
  fork, no new scaffold. Distroless runtime → all volume inspection via the `vol`
  busybox sidecar; never `docker exec`.
- **Allowlist slug (SR-09 / #4975):** A = `arch-research`, B = `isolation-b` —
  both valid under the ADR-004 allowlist `^[a-z0-9][a-z0-9-]{0,62}$`; the ADR is
  authoritative, not a restated regex.
- **Test-scoped slug literal (R-11):** B = `isolation-b`, a neutral name, not a
  real-project-sounding slug — avoids colliding with a pre-existing store on the
  test volume.
- **Single restart (#5079):** register both slugs before the one restart that
  applies `[[projects]]`.
- **Transport identity (#4950):** read each slug's store via its own per-slug
  credentials, or on-disk via the `vol` idiom; never assert one slug's content via
  another slug's credential.
- **Durability barrier first (#5321):** two-store read strictly after the barrier;
  copy `-wal`/`-shm` sidecars; pre-barrier read is INFRA.
- **sqlite3 hard-fail:** provision it like node; absence is INFRA, never an
  empty-pass.

## Open Questions

- **Q1 (RESOLVED — option (a)): Does the MCP-write surface need a second probe,
  and where? → YES, within infra-003.** The human chose option (a): infra-003
  covers **both** the observe surface and the HTTP MCP-write surface in the same
  test (Goal 4, AC-11–AC-15). Rationale, sharpened: observe and MCP-write share the
  same isolation seam (same `parse_project_key`, same `Arc<dyn StoreResolver>`
  instance, per-request), but the MCP path dispatches post-key through a per-slug
  `McpAdapter` holding its own boot-captured store. The `entry.store` ==
  adapter-store equality is guarded only by a `debug_assert!` **compiled out of the
  release container**, so the shipped artifact has **zero behavioral coverage** of
  MCP-write isolation — the MCP probe is the load-bearing half, not a stretch.
  Standing note: N3 (#5161) remains **`partial`** regardless — the N5 (#788)
  regression gate is still unwired; infra-003 does **not** close N3.
- **Q2:** Exact two-store read primitive and marker. The positive control asserts
  the marker is *present*, so the read must be a **content** read (e.g. `sqlite3`
  query for the marker token), not a `du` delta. Which observe payload field
  becomes a queryable marker, and in which table/column (e.g. the `observations`
  table `topic_signal`/feature-token used by the existing behavioral capture, or a
  session row), is a design-session decision.
- **Q3:** Host the test as a new standalone shell gate (mirroring
  `docker-http-posture-smoke.sh`, which the removed D6 probe lived near) or as a
  pytest suite? ass-084 H1 leans shell (reuses the existing `vol`/cert/sqlite3
  idioms directly); H2 (pytest) is deferred.
- **Q4 (RESOLVED):** Slug B = **`isolation-b`** (A = existing `arch-research`). A
  neutral, test-scoped literal — `eval-baseline` was rejected because it reads like
  a live eval-harness project slug and could collide with a pre-existing store on
  the test volume (R-11).
- **Q5:** Register both before a single restart vs. a second restart — both work
  (routing read once at boot); pick the simpler for the harness flow.

## Tracking

- GH Issue: **#853** (this feature).
- Capability: advances **N3 (#5161)**, currently `partial` — point-in-time proof
  on **both** the HTTP observe and MCP-write surfaces; does not close N3 (N5 #788
  regression gate still unwired).
- Related: #845 / #5332 (D6 removed from parity matrix), ass-084 / #850 (GO
  feasibility, folded in), nan-022 (the former D6 home).
- *(Will be updated with the GH Issue link / capability evidence after Session 1.)*
