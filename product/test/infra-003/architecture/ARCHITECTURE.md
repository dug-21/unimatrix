# Architecture: infra-003 — Standalone Multi-Tenant HTTP Isolation Test

> Test-only. Cumulative extension of the infra-001 HTTP posture-smoke harness.
> No production code change, no fork, no new scaffold. Advances capability
> **N3 (#5161)**; does not close it. GH issue: **#853**.

## System Overview

infra-003 adds one **standalone shell gate** that exercises the *shipped*
multi-slug container's cross-tenant isolation through the **real production
routing seam** (`parse_project_key → resolve_store → dispatch`). It is
**bidirectional**: distinctly-marked writes are driven through **both** slugs'
routes on **both** write surfaces, and a genuine **two-store content read**
asserts the full **2×2 matrix** per surface — each store holds **only its own
slug's marker** (present in own, absent in other), in **both** directions.

It does not change routing. The container already multiplexes N slugs in one
process (`MultiProjectRouter`, vnc-038 #5082); this gate **drives** that existing
seam with two real registrations and four real routes, then reads both stores'
content on disk.

Slugs: **A = `arch-research`** (existing), **B = `isolation-b`** (neutral,
test-scoped literal — R-11; never a real-project-sounding name that could collide
with a pre-existing store on the test volume).

Four markers, one per matrix cell, built from a shared per-run nonce `<run>` plus
a disjoint per-cell tag so the four literals are **mutually non-substring**
(load-bearing — see C6/ADR-002, the MCP read is a `LIKE '%marker%'` substring
match):

| Cell | Marker literal |
|------|----------------|
| A · observe | `infra003-obs-a-<run>` |
| B · observe | `infra003-obs-b-<run>` |
| A · MCP | `infra003-mcp-a-<run>` |
| B · MCP | `infra003-mcp-b-<run>` |

Two surfaces, both directions, in the same container, same cert/token/`vol`/`sqlite3`
primitives, **one bearer token** (the slug is in the URL path, so one token serves
all four writes):

| Surface | Routes | Production path | Marker lands in |
|---------|--------|-----------------|-----------------|
| **Observe** | `POST /v1/A/observe`, `POST /v1/B/observe` | `parse_project_key → resolve_store → dispatch_request` | `observations.topic_signal` |
| **MCP-write** | `POST /v1/A/mcp`, `POST /v1/B/mcp` (`context_store`/`context_correct` JSON-RPC) | `parse_project_key → resolve_store`, then per-slug `adapter_for` dispatch | `entries.content` (+ `title`/`topic`/`tags`) |

**Why bidirectional is load-bearing.** A single-direction test (write only through
A, assert A-has-marker / B-empty) cannot catch the symmetric N3 failure — **B's
route mis-resolving into A's store**. If B's writes silently land in A, B's store
stays correctly empty and the negative control passes GREEN on a broken B route.
Route-liveness (non-404) does **not** catch it either: a mis-resolved B route
still responds and runs the handler. Relying on the unit test
`test_two_slugs_route_to_distinct_stores` for the B-direction would contradict the
feature's thesis (prove behavior in the **release artifact**, not via
debug-build/unit construction guarantees). So B's isolation earns the same
behavioral proof as A's.

The MCP surface remains the **load-bearing half**: the `entry.store ==
adapter-store` construction invariant is guarded only by a `debug_assert!`
compiled **out** of the release container (`seam.rs:345`), so the shipped artifact
has **zero behavioral coverage** of MCP-write isolation without this probe
(SR-10) — now exercised in both directions.

## How this fits the larger system

- **HTTP-only property.** Local STDIO/UDS is 1-client:1-project by the ADR-006
  compile-time guard (`FORBIDDEN_IN_LOCAL`); there is no second local route to
  mis-route into. The gate *references* that guard as proof (AC-14), never re-runs
  it, and introduces no parity-matrix shape (the removed D6, #845).
- **Exercises, does not modify.** Slug routing, the resolver, and store binding
  are run as shipped. The only new artifacts are a test script and a host
  provisioning requirement (`sqlite3`).

## Component Breakdown

```
+--------------------------------------------------------------------------+
| HOST (CI runner / dev box) — Docker-capable, node + sqlite3 provisioned   |
|                                                                          |
|  multi-tenant-isolation-smoke.sh  (C7 verdict gate)                       |
|    |                                                                     |
|    |-- C1 read-dependency preflight: assert docker, sqlite3, vol sidecar |
|    |-- C2 two-slug registration + single restart + route-liveness PRECOND|
|    |-- C5 per-cell: WRITE then READ-AS-BARRIER (retry-until-present)      |
|    |     A-obs -> /v1/A/observe ; retry read A.observations until present |
|    |     B-obs -> /v1/B/observe ; retry read B.observations until present |
|    |     A-mcp -> /v1/A/mcp     ; retry read A.entries      until present |
|    |     B-mcp -> /v1/B/mcp     ; retry read B.entries      until present |
|    |-- C6 cross-store negative reads (other store must NOT hold marker)   |
|    |-- C7 verdict: 2x2 matrix per surface, positive-gates-negative        |
|                                                                          |
|   busybox `vol` sidecar  ──ro──>  [ shared docker volume ]               |
|   curl --cacert <pin> ─────────>  :PORT  (container HTTPS, one bearer)   |
+--------------------------------------------------------------------------+
                                        |
              +-------------------------v--------------------------+
              | Distroless container (shipped image, UNCHANGED)    |
              |  PathRouter ─ /health                              |
              |            ├ /v1/{slug}/observe → route_observe    |
              |            └ else → SlugRouter::route_mcp          |
              |  MultiProjectRouter{ arch-research:{store,adapter},|
              |                      isolation-b:{store,adapter} } |
              |  volume: /data/.unimatrix/{arch-research,          |
              |            isolation-b}/unimatrix.db(+wal,+shm)    |
              |          /data/.unimatrix/<hash>/{token,tls/}      |
              +---------------------------------------------------+
```

### C1 — Read-dependency preflight (SR-01, SR-03)

Before any write, assert the read dependencies exist; absence is **INFRA, never a
verdict** (warn+continue is forbidden, #4473):
- Docker available (else SKIP exit 3, matching the posture-smoke contract).
- `sqlite3` present **on the host** (`command -v sqlite3`) — host-side, like
  `node`; the distroless image is never `exec`-ed. Absence → hard INFRA fail
  (AC-11). `sqlite3` is the content-read engine for both surfaces.
- The `vol` busybox sidecar can mount the volume read-only.

`sqlite3` runs **host-side** on the `vol cat`-extracted snapshot — there is no
SQLite in the distroless image and none is added (no production change).

### C2 — Two-slug registration + single-restart orchestration + route-liveness precondition (SR-11, Q5)

Reuses the posture-smoke boot/register/restart idiom:
1. Boot the shipped image (HTTP-on by default); wait for `HTTP transport active`.
2. `project register arch-research` **and** `project register isolation-b` — both
   before the single restart (routing config is read once at boot, #5079;
   `register` creates a store but not routing intent).
3. One `docker restart`; wait for `HTTP transport active` again.
4. **Route-liveness precondition (NOT the verdict):** assert all four routes
   (`/v1/A/observe`, `/v1/B/observe`, `/v1/A/mcp`, `/v1/B/mcp`) respond non-404
   *before any marked write*. This is a **precondition only** — **non-404 ≠
   isolated**. A mis-resolved route still responds and runs the handler, so
   liveness cannot prove isolation; the behavioral verdict is the **bidirectional
   2×2 content read** (C5/C6/C7). Liveness exists to fail loud when a slug never
   built a route at all (the unregistered-B trap), not to certify routing
   correctness. The liveness probe must **not** write any marker (it would pollute
   the stores the verdict reads).

A reuses existing `arch-research`; B is the literal `isolation-b`. Slug literals
are never re-typed copies of the ADR-004 allowlist regex (SR-08 / AC-13).

### C3 — Observe write surface, both directions (SR-04)

Two cert-pinned bearer POSTs over the **one** bearer token:
- `POST /v1/A/observe` carrying a `RecordEvent` with `topic_signal = infra003-obs-a-<run>`.
- `POST /v1/B/observe` carrying a `RecordEvent` with `topic_signal = infra003-obs-b-<run>`.

Each is the genuine `parse_project_key → resolve_store → dispatch_request` funnel;
expect `204`. `topic_signal` round-trips verbatim into `observations.topic_signal`
(`analytics.rs:539-554`), making each marker queryable. The `204` alone is **not**
the verdict — it must pair with the C5 read-as-barrier positive control (a 204
does not prove the write landed in the right store, and the write is not synced
before the 204).

### C4 — MCP-write surface, both directions (SR-10, load-bearing)

Two cert-pinned bearer MCP writes over the same token:
- `POST /v1/A/mcp` — `context_store` whose `content` carries `infra003-mcp-a-<run>`.
- `POST /v1/B/mcp` — `context_store` whose `content` carries `infra003-mcp-b-<run>`.

Each drives the same `parse_project_key → resolve_store` routing, then the
per-slug `adapter_for` dispatch into that slug's `McpAdapter`, whose boot-captured
store must equal its `entry.store`. `context_store` persists an `entries` row;
each marker lands in `entries.content`. rmcp's `StreamableHttpService` requires an
MCP session, so each probe is a small handshake, not a single POST (ADR-003):
`initialize` → its **own** `Mcp-Session-Id` header → `initialized` → `tools/call`.
**Each route uses its own session id; A's session is never reused against B's
route** (a crossed session mis-attributes the very thing under test). The verdict
is the C5/C6 content read of `entries` in both stores, **never** the RPC success
alone (SR-10).

### C5 — Per-cell write + read-as-barrier positive control (SR-02, SR-03; AC-10)

The server write is `tokio::spawn` fire-and-forget with WAL `synchronous=NORMAL`,
so it lands sub-second but is **not** synced before the response. A single
aggregate `store_size` ("store grew") barrier is **unsound** here: a `du` delta is
satisfied the moment the *first* of a store's two writes lands and proves nothing
about the *second* (e.g. `A grew` after `A-obs` says nothing about whether `A-mcp`
is durable), so a content read could race an unsynced write and **false-RED** the
positive control.

The fix: **the positive-control content read is itself the barrier**, keyed to the
specific marker rather than the ambiguous size proxy:
1. Writes are issued **strictly sequentially per store**; each write is immediately
   followed by its own positive-control read (no shared aggregate barrier).
2. The positive-control query is a **bounded retry-until-present**: `vol cat` the
   store (db + `-wal` + `-shm`) and `sqlite3`-query for **this cell's** marker,
   retrying on a deadline-poll until the marker appears. A not-yet-synced write
   simply has not appeared yet → keep polling.
3. **Timeout classifies as INFRA, never RED** (AC-10): if the own marker never
   appears within the deadline, the write's durability could not be established —
   a non-verdict for this direction's positive control, not a property failure.
   (A genuine mis-route still surfaces as **RED** at the C6 cross-store cell, which
   finds the marker in the *wrong* store; see C7.)

This replaces the old single aggregate `store_size` barrier entirely. `store_size`
is retained only for the C2 liveness/boot waits, not as the durability barrier.

### C6 — Cross-store negative read + the two-store read primitive (Q2; SR-01, SR-02, SR-07, SR-10)

The genuine two-store read, parameterized over `(store_dir, table, predicate)`:
1. `vol cat` each per-slug `unimatrix.db` **plus `-wal`/`-shm` sidecars** out to a
   host sandbox (a single-file copy reads a pre-checkpoint false-empty snapshot —
   SR-02). A missing main db is INFRA; a missing already-checkpointed WAL sidecar
   is fine.
2. Host-side `sqlite3` query per marker per store. Presence (own store) is the
   C5 retry-until-present read; **absence (other store) is a single read** after
   that direction's positive control has reached PRESENT:
   - Observe: A's store has `infra003-obs-a-<run>` and **not** `…-obs-b-…`; B's
     store has `…-obs-b-…` and **not** `…-obs-a-…`.
   - MCP: A's store has `infra003-mcp-a-<run>` and **not** `…-mcp-b-…`; B's store
     has `…-mcp-b-…` and **not** `…-mcp-a-…`.
   Illustrative query (observe): `SELECT count(*) FROM observations WHERE
   topic_signal = '<marker>'` (>0 = present). MCP: `SELECT count(*) FROM entries
   WHERE content LIKE '%<marker>%'` — a **substring** match, which is exactly why
   the four markers must be mutually non-substring (ADR-002): a marker that is a
   substring of another would silently `LIKE`-match in the wrong store and
   false-GREEN a real leak.

Each store is read **on disk via the `vol` sidecar** (AC-12 / SR-04) — no slug's
credential is ever used to read another slug's store, and no over-the-wire B read
is needed. Built **fresh**; it does not extend the removed D6 dir-count /
`other_count` heuristic (a latent false-RED trap, ass-084 OoS#2).

### C7 — Verdict gate: bidirectional 2×2, positive-gates-negative (SR-10; AC-05/AC-09)

The central integrity invariant, applied **per surface, per direction**
independently:
- **Positive control gates negative.** A direction's negative (cross-store) cell
  is reported GREEN **only after** its positive control reached PRESENT (C5). A
  "the other store is clean" pass on a write whose own landing was never confirmed
  is forbidden.
- **Own-store positive timeout → INFRA** for that direction (C5), never a vacuous
  pass and never (by itself) RED.
- **Cross-store marker found → RED**, definitively and independently of the
  positive outcome: the marker appearing in the *other* store is a real leak
  (e.g. B mis-resolving into A). This is the cell that converts a mis-route into a
  hard failure even when the own-store positive timed out as INFRA.
- Four **mutually non-substring** markers (`infra003-{obs,mcp}-{a,b}-<run>`, SR-07)
  keep every cell of the matrix independently attributable.
- Three outcomes are discriminated: GREEN (every positive PRESENT in own store,
  every cross-cell absent), RED (a cross-cell marker present), INFRA (read
  dependency / route-liveness precondition unmet, or an own-store positive timed
  out). INFRA and RED are distinct exit states; neither is ever rounded to GREEN.

## Data Flow

```
preflight(docker, sqlite3, vol)                                   [C1]
  → boot image, register A and B, single restart                  [C2]
  → assert all 4 routes non-404 (PRECONDITION, not verdict;       [C2/SR-11]
       no marker written here)
  → run := unique per-run nonce; markers infra003-{obs,mcp}-{a,b}-run

  per cell, SEQUENTIALLY (write then read-as-barrier):            [C5]
    POST /v1/A/observe {topic_signal=infra003-obs-a-run} 204
      → retry read A.observations for that marker until PRESENT
          timeout → INFRA (not RED)
    POST /v1/B/observe {topic_signal=infra003-obs-b-run} 204
      → retry read B.observations until PRESENT (timeout→INFRA)
    MCP handshake /v1/A/mcp (own session); context_store=infra003-mcp-a-run
      → retry read A.entries until PRESENT (timeout→INFRA)
    MCP handshake /v1/B/mcp (own session); context_store=infra003-mcp-b-run
      → retry read B.entries until PRESENT (timeout→INFRA)

  cross-store negative reads (single read each, after own PRESENT):[C6]
    A has obs-b ? yes → RED (B mis-routed into A)
    B has obs-a ? yes → RED (A mis-routed into B)
    A has mcp-b ? yes → RED ;  B has mcp-a ? yes → RED

  verdict per surface:                                            [C7]
    observe: A-obs & B-obs PRESENT, no cross-cell → observe GREEN
    mcp:     A-mcp & B-mcp PRESENT, no cross-cell → mcp GREEN
  → both surfaces GREEN → ALL GATES PASSED
```

## Integration Points with infra-001

- **`vol()` busybox sidecar** (`docker-http-posture-smoke.sh:47`) — all volume
  inspection; never `docker exec` into the distroless image.
- **`store_size()`** (`:55`) — WAL-robust `du -s` over a slug dir; used **only**
  for the C2 boot/liveness waits, **not** as the durability barrier (C5 uses the
  marker-keyed read-as-barrier instead).
- **cert/token pull idiom** (`:441-446`) — `vol cat $HASH_DIR/token` and
  `tls/cert.pem` for the cert-pinned bearer POST; one token serves all four writes
  (slug is in the path).
- **`HASH_DIR` discovery** (`:414`) and **`SLUG_DIR=/data/.unimatrix/{slug}`**
  (`:434`) conventions, applied per slug.
- **`capture_behavioral_topic_signals` content-read idiom**
  (`cloud-bundle-lib.sh:51-97`) — the canonical `vol cat` db+`-wal`+`-shm` →
  host-side `sqlite3 -json` pattern with the **sqlite3-absent hard-INFRA** and
  **WAL-sidecar** discipline. C5/C6 reuse this pattern per store; they do not
  re-implement the copy (SR-02). The retry-until-present wrapper (C5) is the new
  part, modelled on Gate 7's deadline-poll.
- **boot/register/restart + deadline-poll idioms** (`:390-430`, `:322-329`) — C2
  (boot/liveness) and C5 (the retry-until-present loop reuses the deadline-poll
  shape).
- **fail()/exit contract** — the gate folds every failure into a single `fail()`
  (exit 1) for RED, exit 3 for Docker-absent SKIP, and a distinct hard-INFRA fail
  for missing read dependencies / positive-control timeouts (mirrors the nan-019
  ADR-001 append-only exit contract).

Self-containment (SR-12): the new gate is a **separate** top-level script with
**self-contained assertions**. It reuses primitives by sourcing only
define-on-source libs; it does not graft onto the posture-smoke Gates 1–4 flow.
An upstream posture-smoke change therefore surfaces here as an explicit failure,
not a silent skip.

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `parse_project_key(path)` | `fn(&str) -> Result<ProjectKey, RouteError>`; slug = path segment 2 for both `/observe` and `/mcp` | `http/router/seam.rs:199` |
| `ProjectKey::Slug` | `enum ProjectKey { Slug(ProjectSlug) }`; constructible only from transport path | `seam.rs:51` |
| Slug allowlist | `^[a-z0-9][a-z0-9-]{0,62}$` (ADR-004 authoritative; never re-typed); `arch-research`, `isolation-b` both valid | `seam.rs:93` |
| `resolve_store(&key)` | `fn(&ProjectKey) -> Result<Arc<Store>, RouteError>`; pure per-slug `HashMap` lookup, `UnknownProject` otherwise | `project_resolver.rs:204` |
| `adapter_for(&key)` | `fn(&ProjectKey) -> Option<&McpAdapter>`; per-slug adapter, same map as `resolve_store` | `project_resolver.rs:219` |
| store↔adapter equality | `debug_assert!(adapter.wraps_store(&store))` — **compiled out of release** (the SR-10 gap, both directions) | `seam.rs:345` |
| Observe write wire | `HookRequest::RecordEvent { event: ImplantEvent }` (`#[serde(tag="type")]`, `"type":"RecordEvent"`) | `wire.rs:109,129` |
| Observe marker field | `ImplantEvent.topic_signal: Option<String>` → DB column `observations.topic_signal TEXT` | `wire.rs:267`; `db.rs:865` |
| MCP write call | JSON-RPC `tools/call` name `context_store` (or `context_correct`); marker in `content`/`title`/`topic`/`tags` | rmcp `StreamableHttpService` |
| MCP endpoint | `POST /v1/{slug}/mcp`; requires `initialize` → **own** `Mcp-Session-Id` header → `initialized` → `tools/call` (per route, never crossed) | `http/router.rs:299-375` |
| MCP marker columns | `entries.content TEXT`, `entries.title TEXT`, `entries.topic TEXT`, `entry_tags.tag` | `db.rs:541-568` |
| Markers | 4 mutually non-substring literals `infra003-{obs,mcp}-{a,b}-<run>` (shared run nonce + disjoint per-cell tag) | this design / ADR-002 |
| Per-slug store path | `/data/.unimatrix/{slug}/unimatrix.db` (+ `-wal`, `-shm`) for both `arch-research` and `isolation-b` | posture-smoke `:434,462` |
| Path-hash dir (token/cert) | `/data/.unimatrix/<hash>/token`, `.../tls/cert.pem` | posture-smoke `:414,441-443` |
| `vol()` | `docker run --rm -v "$VOL:/data:ro" busybox "$@"` | posture-smoke `:47` |
| `store_size(dir)` | `vol du -s "$dir" | awk '{print $1}'` (WAL-robust) — C2 liveness waits only, NOT the barrier | posture-smoke `:55` |
| content read | `vol cat db (+-wal/-shm)` → host `sqlite3 -json "$tmp" "<query>"`; sqlite3-absent = hard INFRA; positive read = retry-until-present | `cloud-bundle-lib.sh:51-97` |

## How each top risk is structurally addressed

| Risk | Structural mitigation | Component |
|------|------------------------|-----------|
| **SR-10** MCP `debug_assert` compiled out → zero shipped coverage | MCP probe is a real `context_store` write + **content read of `entries`** in **both directions** (own session each), same rigor as observe; never a `du` delta or success-RPC-only check | C4, C5, C6, C7 |
| **SR-11** single-restart ordering / B route | Register **both** A and B before the one restart; route-liveness is a **precondition** (catches unregistered B), and the **bidirectional 2×2 read** is the verdict that catches a B route mis-resolving into A | C2, C6, C7 |
| **SR-01** sqlite3/`vol` absent → silent empty-pass | Preflight presence-assert; absent sqlite3 = hard INFRA (provisioned like node), never empty capture | C1, C5, C6 |
| **SR-02** WAL sidecars not copied → false-empty | `vol cat` copies db **+ `-wal`/`-shm`** per store, reusing the `cloud-bundle-lib` idiom verbatim | C5, C6 |
| **SR-03** durability barrier unsound / pre-barrier read mistaken for verdict | The positive-control read **is** the barrier — a marker-keyed retry-until-present; per-write (not aggregate `store_size`); own-store timeout = INFRA, never RED | C5, C7 |
| **SR-04** two-slug credential surface / cross-credential read | Each store read **on-disk via `vol`** (no over-the-wire B read, no cross-credential); one bearer token only authorizes the writes, slug is in the path | C3, C4, C6 |
| **SR-07** marker collision / substring false-match | **Four mutually non-substring** markers (`infra003-{obs,mcp}-{a,b}-<run>`); a `LIKE '%marker%'` read cannot silently match across cells | C3, C4, C6, C7 |
| **SR-12** cumulative coupling to Gates 1–4 | Separate script, self-contained assertions; sources only define-on-source primitives | C7 |
| SR-05 overclaim N3 | Gate proves point-in-time only; N3 stays `partial` (documented; N5 gate unwired) | — |
| SR-06 parity reintroduction | No UDS probe, no parity-matrix entry; ADR-006 guard referenced not re-run | — |
| SR-08 allowlist drift | A reuses `arch-research`; B is the literal `isolation-b`; ADR-004 regex never re-typed | C2 |

## Open Questions

- **Construction detail (for spec/tester):** the exact `context_store` JSON-RPC
  frames and the streamable-HTTP per-route `initialize`/`Mcp-Session-Id` handshake
  for C4 (ADR-003 fixes the *approach*, that it runs in both directions, and that
  each route uses its own session; the literal frames are a tester implementation
  detail). Confirm whether `context_store` or `context_correct` is the chosen MCP
  write verb (either persists an `entries` row carrying the marker; default
  recommendation: `context_store`).

(Q4 — slug B literal — is **resolved**: `isolation-b`.)

## ADRs

- ADR-001 — Host as a standalone shell gate reusing infra-001 primitives (Q3)
- ADR-002 — Two-store content read: read-as-barrier retry model + non-substring four-marker 2×2 (Q2)
- ADR-003 — Bidirectional MCP-write probe over streamable HTTP, per-route session isolation, load-bearing
- ADR-004 — Single-restart two-slug registration; route-liveness precondition, bidirectional content-read verdict (Q5)
