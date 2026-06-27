# infra-003 Implementation Brief

> Test-only feature. Standalone multi-tenant HTTP cross-tenant isolation test —
> **bidirectional 2×2** across two write surfaces. Cumulative extension of the
> infra-001 HTTP posture-smoke harness (no fork, no scaffold, no production-code
> change). Advances capability **N3 (#5161 — writes never mis-routed across
> projects)** across the HTTP observe **and** MCP-write surfaces, in **both
> directions**; **does not close it** (N5 #788 regression gate stays unwired). GH
> issue **#853**.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/test/infra-003/SCOPE.md |
| Scope Risk Assessment | product/test/infra-003/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/test/infra-003/architecture/ARCHITECTURE.md |
| Specification | product/test/infra-003/specification/SPECIFICATION.md |
| Risk / Test Strategy | product/test/infra-003/RISK-TEST-STRATEGY.md |
| Alignment Report | product/test/infra-003/ALIGNMENT-REPORT.md |
| ADR-001 (shell gate hosting) | product/test/infra-003/architecture/ADR-001-shell-gate-hosting.md |
| ADR-002 (read-as-barrier + non-substring 4-marker 2×2) | product/test/infra-003/architecture/ADR-002-two-store-content-read-and-markers.md |
| ADR-003 (bidirectional MCP probe, per-route session isolation) | product/test/infra-003/architecture/ADR-003-mcp-write-probe-construction.md |
| ADR-004 (single-restart + route-liveness precondition) | product/test/infra-003/architecture/ADR-004-single-restart-registration-ordering.md |

## Goal

Prove behaviorally — in the **release container**, not a unit test — that an HTTP
write addressed to a slug lands **only** in that slug's per-slug store, across two
served write surfaces (observe `POST /v1/{slug}/observe`; HTTP MCP-write
`POST /v1/{slug}/mcp`) and in **both directions** between tenants A
(`arch-research`) and B (`isolation-b`). The gate drives four distinctly-marked
writes through the genuine production funnel (`parse_project_key → resolve_store →
dispatch`) and, via a genuine two-store content read, asserts the full
discrimination matrix per surface: each store holds **only its own** marker
(present in own, absent from other), both directions. The B-direction is
load-bearing — a single-direction test passes GREEN on B's route mis-resolving
into A's store (B-on-disk reads correctly empty). Point-in-time, test-only, no
production change.

## Component Map

The deliverable is a single standalone shell gate composed of seven logical
components (C1–C7, ADR-001). Pseudocode and test-plan files are produced in
Session 2 Stage 3a; paths below are the expected layout.

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| C1 — Read-dependency preflight (docker / sqlite3 / vol) | pseudocode/c1-preflight.md | test-plan/c1-preflight.md |
| C2 — Two-slug registration + single restart + route-liveness **precondition** | pseudocode/c2-registration.md | test-plan/c2-registration.md |
| C3 — Observe writes, both directions (`obs-a`, `obs-b`) | pseudocode/c3-observe-probe.md | test-plan/c3-observe-probe.md |
| C4 — MCP-write probe, both directions (`mcp-a`, `mcp-b`; per-route own `Mcp-Session-Id`, load-bearing) | pseudocode/c4-mcp-probe.md | test-plan/c4-mcp-probe.md |
| C5 — Per-cell write + **read-as-barrier** positive control (retry-until-present) | pseudocode/c5-read-as-barrier.md | test-plan/c5-read-as-barrier.md |
| C6 — Cross-store negative read + two-store read primitive (non-substring 2×2) | pseudocode/c6-two-store-read.md | test-plan/c6-two-store-read.md |
| C7 — Verdict gate (bidirectional 2×2, per-direction positive-gates-negative, tri-state exit) | pseudocode/c7-verdict.md | test-plan/c7-verdict.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Resolved Decisions

| Decision | Resolution | Source | ADR File (Unimatrix ID) |
|----------|-----------|--------|-------------------------|
| Q1 — Does the MCP-write surface need a probe, and where? | YES, in infra-003 — both observe + MCP-write, both directions, in one test. MCP is the load-bearing half (`debug_assert!` store↔adapter guard compiled out of release). | SCOPE Q1, Goal 4 | architecture/ADR-003-mcp-write-probe-construction.md (#5343) |
| Bidirectional 2×2 | Four distinctly-marked writes through both slugs on both surfaces; verdict is the full matrix per surface (each store holds only its own marker, both directions). Closes the symmetric N3 failure (B mis-resolving into A) a one-directional test passed GREEN on. | SCOPE / SPEC / ARCH | architecture/ADR-002-two-store-content-read-and-markers.md (#5342), ADR-003 (#5343) |
| Durability barrier model | **No aggregate `store_size` ("store grew") barrier** — unsound when a store takes two writes. The positive-control content read **is** the barrier: a marker-keyed, bounded **retry-until-present** loop. Own-store timeout → INFRA (never RED); a genuine mis-route surfaces RED at the cross-store cell. `store_size` demoted to liveness/boot waits only. | SPEC FR-06, ARCH C5 | architecture/ADR-002-two-store-content-read-and-markers.md (#5342) |
| Q2 — Read primitive + markers | One parameterized `vol cat` (db+`-wal`+`-shm`) → host-side `sqlite3` content read. Four **mutually non-substring** markers (see Data Structures). | SCOPE Q2 | architecture/ADR-002-two-store-content-read-and-markers.md (#5342) |
| MCP per-route session isolation | Each `/v1/{slug}/mcp` probe runs its own handshake and captures/uses its **own** `Mcp-Session-Id`; A's session is never reused against B's route (a crossed session mis-attributes the test). New AC-15. | SPEC AC-15/FR-07.3, ARCH C4 | architecture/ADR-003-mcp-write-probe-construction.md (#5343) |
| Q3 — Host as shell gate or pytest? | Standalone shell gate `multi-tenant-isolation-smoke.sh` reusing infra-001 primitives; pytest (H2) rejected as new scaffold. | SCOPE Q3 | architecture/ADR-001-shell-gate-hosting.md (#5335) |
| Q4 — Slug B literal | **`isolation-b`** — neutral, test-scoped, allowlist-valid; `eval-baseline` rejected (collision risk R-11). DECIDED. | SCOPE Q4, SPEC C-04 | architecture/ADR-004-single-restart-registration-ordering.md (#5344) |
| Q5 — Single vs second restart | Single restart: register both A and B before the one restart; route-liveness is a **precondition only** (non-404 ≠ isolated), the 2×2 content read is the verdict. | SCOPE Q5 | architecture/ADR-004-single-restart-registration-ordering.md (#5344) |
| MCP write verb | Default `context_store` (simplest single-row marker); `context_correct` permitted but needs a prior entry. | ADR-003 | architecture/ADR-003-mcp-write-probe-construction.md (#5343) |

## Files to Create / Modify

| Path | Change | Summary |
|------|--------|---------|
| `product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh` | CREATE | The standalone bidirectional isolation gate (C1–C7); separate top-level script alongside `docker-http-posture-smoke.sh`, self-contained assertions (ADR-001, SR-12). |
| New-smoke-script invariant test (#815) | MODIFY (same PR) | Register `multi-tenant-isolation-smoke.sh` as a known/expected smoke script so the invariant does not trip; keep its guard against unaccounted future scripts. Ships **in the same delivery PR** as the script, cross-linked on #815 (R-15 — DELIVERY ACTION). |
| Host/CI provisioning | MODIFY (config, not crates) | Provision `sqlite3` on the host like `node`; presence asserted in C1, absence is INFRA. |

No `crates/` change. AC-13 is verified by `git diff` showing no production-code
edit. Existing infra-001 libs (`docker-http-posture-smoke.sh`,
`cloud-bundle-lib.sh`) are **sourced for primitives only**, not modified.

## Data Structures / Wire Formats

- **Slugs:** A = `arch-research` (existing constant, not re-typed), B =
  `isolation-b` (neutral test-scoped literal). Both valid under the ADR-004
  allowlist `^[a-z0-9][a-z0-9-]{0,62}$` (`seam.rs:93`), authoritative and never
  re-typed into the harness.
- **Four mutually non-substring markers** — a shared per-run nonce `<run>` (e.g.
  PID + timestamp) plus a **disjoint per-cell tag**, differing at the `obs/mcp` and
  `a/b` positions before the shared suffix:
  - `infra003-obs-a-<run>` → A observe → `observations.topic_signal`
  - `infra003-obs-b-<run>` → B observe → `observations.topic_signal`
  - `infra003-mcp-a-<run>` → A MCP → `entries.content` (+ `topic`)
  - `infra003-mcp-b-<run>` → B MCP → `entries.content` (+ `topic`)
  **Mutual non-substring is load-bearing** (R-18): the MCP read is
  `content LIKE '%<marker>%'`, so if one marker were a substring of another it
  would false-match a cross-direction negative control and pass GREEN on a real
  leak. "Distinct" is insufficient. Charset constrained to `[a-z0-9-]` (no
  SQL/LIKE metacharacters, R-12); per-run nonce isolates runs on a reused volume
  (NFR-05, R-08). Delivery MUST use these literals (or another provably
  mutually-non-substring set).
- **One bearer token, slug in URL path** — identity comes from the path
  (`ProjectKey::Slug`, never payload, #4950); one token authorizes all four writes
  (token authorizes the caller, path selects the tenant). Per-slug read identity is
  on-disk via the `vol` sidecar.
- **Observe wire:** `HookRequest::RecordEvent { event: ImplantEvent }`
  (`#[serde(tag="type")]`, `"type":"RecordEvent"`); marker in
  `ImplantEvent.topic_signal: Option<String>` → `observations.topic_signal TEXT`.
- **MCP wire:** JSON-RPC over streamable HTTP, **per direction, own session**.
  Handshake per route: `initialize` → capture **that route's** `Mcp-Session-Id` →
  `notifications/initialized` → `tools/call` `context_store` (marker in `content`/
  `topic`). `Accept: application/json, text/event-stream` required (rmcp forces
  SSE, #5296/#5129); response is SSE-framed. `Mcp-Session-Id` is a per-session UUID
  minted at `initialize` (#4708) — **never reused across routes** (R-17).
- **Per-slug store path:** `/data/.unimatrix/{slug}/unimatrix.db` (+ `-wal`,
  `-shm`) for both `arch-research` and `isolation-b`. **Path-hash dir:**
  `/data/.unimatrix/<hash>/token`, `.../tls/cert.pem`.

## Durability Barrier — read-as-barrier (load-bearing model change)

The writes are `tokio::spawn` fire-and-forget under `synchronous=NORMAL`, so a
`204`/RPC-success returns before the write is synced. The earlier aggregate
`store_size` ("A grew AND B grew") barrier is **removed** — it is satisfied by the
*first* of a store's two writes and proves nothing about the second, so a content
read could race an unsynced write and false-RED the positive control. Instead:

1. The four writes are issued **strictly sequentially per store**; each write is
   immediately followed by its own positive-control read (no shared aggregate
   barrier).
2. The positive-control query is a **bounded retry-until-present** loop — `vol cat`
   the store (db + `-wal` + `-shm`) and `sqlite3`-query for *that cell's* marker,
   retrying on a deadline-poll until the marker appears. The marker becoming
   queryable IS the durability proof.
3. **Own-store timeout → INFRA** (never RED, never a vacuous pass). A genuine
   mis-route still surfaces as **RED** at the cross-store negative cell (the marker
   found in the *wrong* store), independent of the positive outcome.
4. `store_size` is retained **only** for C2 boot/liveness waits, never as the
   durability barrier.

## Key Interfaces (production seam — exercised, not modified)

| Symbol | Signature | Source |
|--------|-----------|--------|
| `parse_project_key(path)` | `fn(&str) -> Result<ProjectKey, RouteError>`; slug = path segment 2 | `http/router/seam.rs:199` |
| `ProjectKey::Slug` | `enum ProjectKey { Slug(ProjectSlug) }`; from transport path only | `seam.rs:51` |
| Slug allowlist | `^[a-z0-9][a-z0-9-]{0,62}$` (ADR-004 authoritative; never re-typed) | `seam.rs:93` |
| `resolve_store(&key)` | `fn(&ProjectKey) -> Result<Arc<Store>, RouteError>`; per-slug HashMap lookup | `project_resolver.rs:204` |
| `adapter_for(&key)` | `fn(&ProjectKey) -> Option<&McpAdapter>`; same map as resolve_store | `project_resolver.rs:219` |
| store↔adapter equality | `debug_assert!(adapter.wraps_store(&store))` — **compiled out of release** (SR-10 gap, both directions) | `seam.rs:345` |
| MCP endpoint | `POST /v1/{slug}/mcp`; `initialize` → **own** `Mcp-Session-Id` → `initialized` → `tools/call` (per route, never crossed) | `http/router.rs:299-375` |
| `vol()` | `docker run --rm -v "$VOL:/data:ro" busybox "$@"` | posture-smoke `:47` |
| `store_size(dir)` | `vol du -s "$dir"` (WAL-robust; #5193) — **C2 liveness waits only, NOT the barrier** | posture-smoke `:55` |
| content read | `vol cat db(+-wal/-shm)` → host `sqlite3 -json`; sqlite3-absent = hard INFRA; positive read = retry-until-present | `cloud-bundle-lib.sh:51-97` |

## Constraints

- **C-01 test-only / cumulative:** extend infra-001; no production change, fork, or new scaffold.
- **C-02 distroless:** all volume inspection via the `vol` busybox sidecar; never `docker exec`.
- **C-03 allowlist (SR-08/#4975):** A = `arch-research`, B = `isolation-b`; both valid under ADR-004 regex; ADR authoritative, never re-typed.
- **C-04 test-scoped slug literal (R-11):** B = `isolation-b`, a neutral name, not a real-project-sounding slug.
- **C-05 single restart (#5079):** register both slugs before the one restart.
- **C-06 route-liveness ≠ verdict:** a non-404 route is a precondition (catches unregistered B); the isolation verdict is the content-read 2×2 matrix only.
- **C-07 transport identity (#4950):** read each slug's store on-disk via `vol`, or with its own credentials; one slug's credential never reads another's; the shared write token is never repurposed as a read proof.
- **C-08 sound per-write barrier, no aggregate `du` (#5321):** strictly sequential per-store writes; the positive-control marker-keyed read is its own barrier via a bounded retry-until-present (pre-deadline miss = INFRA/retry; at-deadline own-store miss = INFRA, durability not established, never RED; wrong-store presence = RED). No "A grew AND B grew" aggregate `store_size` barrier.
- **C-09 sqlite3 hard-fail (SR-01):** provision like node; absence = INFRA, never empty-pass.
- **C-10 four distinct markers (SR-07):** one per (slug, surface) cell; observe in `observations`, MCP in `entries`.
- **C-11 no count heuristics:** content reads only; the removed D6 `du`/dir-count/`other_count` logic is replaced, not extended (ass-084 OoS#2).
- **C-12 tri-state exit:** GREEN / RED (exit 1) / INFRA (distinct) / SKIP (exit 3, docker absent); no non-pass ever rounds to exit 0 (#5180, R-10).
- **C-13 MCP per-session isolation (R-17):** each `/v1/{slug}/mcp` probe captures and uses its own `Mcp-Session-Id`; never reuse one slug's session against another's route.
- **C-14 mutually non-substring markers (SR-07/R-18):** the four markers must be mutually non-substring (shared `<run>`-nonce + disjoint per-cell tag), because the MCP read uses `LIKE '%marker%'`.
- **C-15 N3 partial (SR-05):** capability evidence = "advances, does not close N3"; N5/#788 adoption (durable #788 linkage) named as the path to "maintained" (R-16).

## Dependencies

| Dependency | Purpose |
|-----------|---------|
| Docker (distroless runtime image) | Build + run the shipped multi-slug container under test |
| busybox image | `vol` sidecar for read-only volume inspection |
| `sqlite3` (host) | Content-read engine; **provisioned like node**, absence = INFRA (AC-11) |
| `curl` | Cert-pinned bearer POST to the four routes (one token, slug in path) |
| `node` | JSON shaping of `sqlite3 -json` output / payload assembly |
| `unimatrix-server` image | Shipped multi-slug container (`MultiProjectRouter`, vnc-038 #5082) — exercised, not modified |
| infra-001 harness | `docker-http-posture-smoke.sh`, `cloud-bundle-lib.sh` — sourced for primitives (`vol()`, cert/token pull, WAL-aware `vol cat`, `capture_behavioral_topic_signals`, sqlite3 hard-fail, deadline-poll shape for the retry loop) |
| New-smoke-script invariant test (#815) | Updated in the same PR to register the new script (R-15 DELIVERY ACTION) |
| N5 / #788 standing regression lane | Adoption target (durable #788 linkage) so the point-in-time proof advances to "maintained" (R-16 DELIVERY ACTION) |
| ADR-006 guard | `FORBIDDEN_IN_LOCAL` / `local_binding_guard_tests.rs` — **referenced** as proof, not re-run (AC-14) |

## NOT in Scope

- Claiming N3 (#5161) proven/closed; N3 stays `partial`.
- Wiring the N5 (#788) standing regression gate's mechanics (only its **adoption** of this gate is a delivery action — R-16).
- Any UDS behavioral probe — ADR-006 `FORBIDDEN_IN_LOCAL` is referenced, never re-run.
- Any parity-matrix entry / probe-for-probe parity with the UDS leg (removed D6, #845).
- Production routing/resolver/store-binding change — exercised as shipped.
- A new pytest suite / H2 orchestrator dual-leg capture (ass-084 H2 deferred).
- Reusing/extending the removed D6 `capture_isolation_probe` dir-count/`other_count` logic.
- Treating route-liveness (non-404) as the isolation verdict — precondition only.
- An aggregate `store_size` durability barrier — removed as unsound; replaced by the read-as-barrier.
- Over-the-wire read-back of any entry (`context_search`/`context_get`) as a positive control — on-disk content read is the authoritative verdict.

## Alignment Status

Vision Guardian: **PASS** on Vision Alignment, Milestone Fit, Scope Gaps,
Architecture Consistency, Risk Completeness. The single prior WARN — slug B
literal — is **RESOLVED** (design adopted `isolation-b`, the guardian's recommended
option). No open variances require a human decision.

- The MCP SSE handshake (now per-direction with per-route session isolation), the
  read-as-barrier durability model, and the non-substring markers are *required*
  soundness mechanisms (not gold-plating); they are Critical/High risk surfaces
  tracked in the risk strategy (R-01, R-05, R-17, R-18).

## Delivery Actions (carry into Session 2)

1. **R-16 (standing-gate orphan) — HIGH, concrete linkage.** This is a
   point-in-time gate; N3 stays `partial` because the N5/#788 recurring lane is
   unwired. Per the design a **durable adoption comment is posted on #788**
   requiring N5/#788 to adopt infra-003's gate into the recurring lane (run on N5's
   cadence), advancing N3 from point-in-time toward "maintained." Delivery must
   ensure this linkage is present/durable (not an informal note) and that capability
   evidence names the #788 adoption as the path to "maintained."
2. **R-15 (new-smoke-script invariant / #815) — HIGH, in-PR lockstep.** ADR-001
   adds a new top-level smoke script that will trip the known new-smoke-script
   invariant test (open via #815 since #810 added a second script). The invariant
   update lands in the **same delivery PR** as the new script (not a follow-up),
   **cross-linked on #815** (issue comment), registering the script as expected
   while keeping the guard against unaccounted future scripts; #815's intent is
   closed in that change. Honor the verify-by-name / exit-code contract (#5180).

## Open Items (for human review)

None. Slug B is decided (`isolation-b`); the prior alignment WARN is resolved.

## Delivery Notes (non-blocking)

- **AC numbering — acceptance map is authoritative (carry the full 15).** SPEC and
  ACCEPTANCE-MAP carry **AC-01…AC-15** (AC-15 = MCP per-session isolation, grouped
  with the MCP-write surface). SCOPE's higher-level list is AC-01…AC-14 — a known,
  non-blocking drift. The ACCEPTANCE-MAP is the authoritative AC tracker.
- **R-05 residual:** the read-as-barrier loop must be a bounded deadline-poll (no
  unbounded hang, no fixed `sleep` substituted for the poll) with correct
  INFRA-vs-RED discrimination (own-store absence-timeout = INFRA; wrong-store
  presence = RED), and the cross-store negative evaluated only after the positive
  reaches PRESENT. Confirm the deadline value against arm64-CI headroom (mirror the
  existing ~10s store-grow wait in `docker-http-posture-smoke.sh`).
- **Spec is authoritative on query form.** ARCHITECTURE C6 uses illustrative
  `SELECT count(*) … >0` / `content LIKE '%marker%'`; SPEC AC-03/AC-07 are the
  canonical forms (presence assertions; AC-07 broadens MCP to
  `content LIKE … OR topic = …`). Cosmetic; not a blocker.
- **ALIGNMENT-REPORT.md is stale** relative to this revision (still references the
  prior single-direction AC-01…AC-15 framing with `eval-baseline` and the slug-B
  WARN). The underlying variance is resolved by the design's adoption of
  `isolation-b`. Regenerating the alignment report is tidy-up, not a gate blocker.
