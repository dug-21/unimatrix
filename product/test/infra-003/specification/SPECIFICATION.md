# Specification: infra-003 — Standalone Multi-Tenant HTTP Isolation Test

> Test-only feature. Cumulative extension of the infra-001 HTTP integration
> harness (no fork, no scaffold). Advances capability **N3 (#5161 — writes never
> mis-routed across projects)**; does **not** close it. GH issue **#853**.

## Objective

Prove behaviorally — in the **release container**, not a unit test — that an HTTP
write addressed to a slug lands **only** in that slug's per-slug store, across two
served write surfaces (observe `POST /v1/{slug}/observe`; HTTP MCP-write
`POST /v1/{slug}/mcp`, `context_store`/`context_correct`) and in **both
directions** between two tenants A (`arch-research`) and B (`isolation-b`). The
test drives four distinctly-marked writes (A-observe, B-observe, A-mcp, B-mcp)
through the genuine production funnel (`parse_project_key → resolve_store →
dispatch_request`) and, via a genuine two-store content read, asserts the full
discrimination matrix per surface: each store holds **only its own** marker
(present in its own store, absent from the other), in both directions. This is a
point-in-time, test-only proof with no production-code change.

---

## Domain Models / Ubiquitous Language

| Term | Definition |
|------|------------|
| **Surface** | A served write entry point under test. Two surfaces: **observe** (`POST /v1/{slug}/observe`) and **MCP-write** (`POST /v1/{slug}/mcp`, `context_store`/`context_correct`). |
| **Slug A** | Tenant A. Reuses the existing allowlist-valid slug `arch-research` (no re-typed allowlist literal). |
| **Slug B** | Tenant B. Allowlist-valid, neutral, **test-scoped** literal **`isolation-b`** (Q4 / R-11) — deliberately not a real-project-sounding name, to avoid colliding with a pre-existing store on the test volume. |
| **Marker** | A unique, test-generated string written through one (slug, surface) cell and later queried by exact/substring match in a specific SQLite column. **Four mutually non-substring markers**, one per cell of the 2×2 matrix. |
| **Run-nonce** | A single per-run identifier `<run>` (e.g. PID + timestamp) shared by all four markers, making every marker unique per run so a stale store artifact cannot satisfy a control. |
| **The four markers** | Built from a shared `<run>`-nonce + a disjoint per-cell tag so the four literals are **mutually non-substring**: `A-obs` = `infra003-obs-a-<run>` (A-observe → `observations.topic_signal`); `B-obs` = `infra003-obs-b-<run>` (B-observe → `observations.topic_signal`); `A-mcp` = `infra003-mcp-a-<run>` (A-mcp → `entries.content`/`entries.topic`); `B-mcp` = `infra003-mcp-b-<run>` (B-mcp → `entries.content`/`entries.topic`). Shorthand `A-obs`/`B-obs`/`A-mcp`/`B-mcp` denotes these literals throughout. **Non-substring is load-bearing:** the MCP read is `content LIKE '%<marker>%'`, so if any marker were a substring of another it would false-match a cross-direction negative control and pass GREEN on a real leak (Q2, SR-07). |
| **Two-store read** | Reading **both** A's and B's per-slug `unimatrix.db` for a marker via the `vol` busybox sidecar + `sqlite3 -json`, after the durability barrier, with `-wal`/`-shm` sidecars copied. Never a directory size delta or `other_count`/dir-count heuristic. |
| **Bidirectional / 2×2 matrix** | For each surface, the verdict is a 2×2 grid: A-store×{A-marker present, B-marker absent} and B-store×{B-marker present, A-marker absent}. Single-direction (write only through A) cannot detect B's route mis-resolving into A's store — B-on-disk reads correctly empty and the negative control passes GREEN on a broken B route. |
| **Positive control** | The load-bearing assertion that a store **contains its own** marker after the write. A bare `204`/RPC-success or `du` delta is **not** a positive control. |
| **Negative control (cross-contamination)** | The assertion that a store does **not** contain the *other* slug's marker. |
| **Positive-gates-negative (per direction)** | If a direction's positive control fails (own marker absent from own store), that direction fails **RED** and the negative control's "other store is clean" result is **never** reported as a pass. A silently-failed own-write must never produce a vacuous other-empty GREEN. |
| **Route-liveness** | A route responding non-404 after registration. A **precondition only**, never the isolation verdict — a mis-resolved route still responds non-404. |
| **Durability barrier** | The per-store, per-write guarantee that a specific marker's row is durable and readable before its verdict is taken (#5321). The writes are `tokio::spawn` fire-and-forget under `synchronous=NORMAL`, so a `du` size delta is **not** a sound barrier: a store's `du` grows on the *first* of its two writes and says nothing about the *second* write's durability, and an unsynced write races the content read. **There is no single aggregate barrier** ("A grew AND B grew" after all four writes). Instead, writes are strictly sequential per store, and **the marker-keyed content read is itself the barrier** — see Marker-keyed retry-until-present. |
| **Marker-keyed retry-until-present** | Each positive-control query is a bounded **retry-until-present** loop with a deadline: it re-queries the store for the *own* marker until the row appears. The marker becoming queryable IS the durability proof (not the `du` size proxy). Marker-not-yet-present **before** the deadline is **INFRA (retry/keep-waiting)**; marker still absent **at** the deadline is **also INFRA** — a durability/infra failure (the own-write never became durable), **never RED**. A genuine mis-route surfaces as **RED at the cross-store (negative-control) cell** (wrong-store presence), never as an own-store positive-control timeout. The negative control is evaluated only **after** the own-store positive is confirmed present. |
| **INFRA-fail** | A precondition error (absent `sqlite3`; absent `vol` sidecar; missing main `unimatrix.db`; missing/uncopied WAL sidecars; pre-barrier or stale-barrier read; a route absent post-restart) that aborts the gate as a mis-provisioned/mis-ordered condition — **never** a verdict and **never** a silent empty-pass. `warn+continue` is forbidden (#4473). |

### Production seam exercised (not modified)

```
HTTP request → parse_project_key (seam.rs:199-215, path segment 2, allowlist-validated at seam.rs:86)
             → ProjectKey::Slug (constructible only from transport path, never payload)
             → resolve_store (MultiProjectRouter::resolve_store, project_resolver.rs:204;
                              per-request HashMap lookup; UnknownProject otherwise — never a default, never another slug)
observe:     → dispatch_request(resolved Arc<Store>) → insert_observation → observations table
mcp-write:   → adapter_for(key) → per-slug McpAdapter (boot-captured store) → entries table
```

Both surfaces share the **same** `parse_project_key` and the **same** `Arc<dyn
StoreResolver>` instance (per request). MCP-write diverges post-key through a
per-slug `McpAdapter` whose `entry.store == adapter-store` equality is guarded
only by a `debug_assert!` **compiled out of the release container** — hence the
MCP content read is the load-bearing half, not a stretch.

---

## Functional Requirements

Each requirement is testable; verification methods appear in the Acceptance
Criteria table.

### FR-01: Dual-slug registration before a single restart
- FR-01.1: Register **both** slug A (`arch-research`) and slug B (`isolation-b`) **before** the one restart that applies `[[projects]]` (#5079: routing read once at boot).
- FR-01.2: After restart, assert all four routes (`/v1/A/observe`, `/v1/B/observe`, `/v1/A/mcp`, `/v1/B/mcp`) respond (a registered slug builds a `ProjectEntry` at boot) **before** any marked write (SR-11).
- FR-01.3: Route-liveness (non-404) is a **precondition only**, never the isolation verdict — a mis-resolved route still responds non-404 (SCOPE AC-01).
- FR-01.4: Slug literals are not re-typed copies of the ADR-004 allowlist value; A reuses the existing constant, B is the single neutral test-scoped literal `isolation-b` (SR-08, R-11).

### FR-02: Four distinctly-marked writes through the real funnels
- FR-02.1: Drive four writes over the existing cert-pinned bearer path; the slug is in the URL path, so **one bearer token serves all four** writes.
- FR-02.2: **Observe, both directions:** `POST /v1/A/observe` carrying marker `A-obs` in the payload's `topic_signal` field, and `POST /v1/B/observe` carrying `B-obs` in `topic_signal` — the genuine `parse_project_key → resolve_store → dispatch_request` funnel; each returns `204`.
- FR-02.3: **MCP-write, both directions:** `POST /v1/A/mcp` carrying marker `A-mcp` in `content` (and `topic`), and `POST /v1/B/mcp` carrying `B-mcp` in `content` (and `topic`) — `context_store`/`context_correct` JSON-RPC, tool name in the body; each returns a JSON-RPC success response.
- FR-02.4: All four markers are distinct literals; observe markers land in `observations.topic_signal`, MCP markers land in `entries.content`/`entries.topic` (Q2). A `204`/RPC-success alone is **not** a landing proof; it must be paired with the content read (FR-03).

### FR-03: Genuine two-store content read (per surface, per direction)
- FR-03.1: For each marker, read **both** A's and B's `unimatrix.db` via `vol cat` + `sqlite3 -json`, with `-wal`/`-shm` sidecars copied alongside each main db (SR-02).
- FR-03.2: Reads are content reads — exact/substring match on the marker column — never a `du` size delta, dir-count, or `other_count`/heuristic (Design Recommendation 2; ass-084 OoS#2).
- FR-03.3: Each slug's store is read via that slug's **own per-slug credentials** (over the wire) or via an on-disk `vol` read; one slug's credential is never reused to read another's store (#4950, SR-04).

### FR-04: Bidirectional matrix verdict (per surface)
- FR-04.1: **Observe:** A's store contains `A-obs` **and not** `B-obs`; B's store contains `B-obs` **and not** `A-obs`.
- FR-04.2: **MCP-write:** A's store contains `A-mcp` **and not** `B-mcp`; B's store contains `B-mcp` **and not** `A-mcp`.
- FR-04.3: The B-direction (B's own-marker present, A's marker absent from B; and A's store free of B's marker) is the cell that catches **B's route mis-resolving into A's store** — the symmetric N3 failure a single-direction test misses.

### FR-05: Positive-gates-negative, per direction
- FR-05.1: For each (slug, surface) direction, evaluate the positive control (own marker present in own store) first.
- FR-05.2: If a direction's positive control fails, that direction fails **RED**; the cross-contamination "other store is clean" result is **not** reported as a pass (no vacuous pass on a silently-failed own-write) (SR-10).
- FR-05.3: A surface passes only when **both** directions' positive controls are GREEN **and** both negative (cross-contamination) controls are GREEN.
- FR-05.4: Every emitted verdict cites a check (the marker query result) that would **FAIL** if isolation broke — never a size/count heuristic (#5177, #2758).

### FR-06: Sound per-write durability barrier (no aggregate `du` barrier)
- FR-06.1: The four writes are issued **strictly sequentially per store**; there is **no single aggregate barrier** ("A grew AND B grew" after all four). A `du` size delta is not a sound barrier — it is satisfied by the first of a store's two writes and says nothing about the second's durability; under `tokio::spawn` fire-and-forget + `synchronous=NORMAL` the content read otherwise races an unsynced write and the positive control false-REDs (#5321, SR-03).
- FR-06.2: Each positive-control content query is a **bounded retry-until-present** loop with a deadline; the **marker-keyed read is its own barrier**. Marker-not-yet-present **before** the deadline classifies as **INFRA (retry/keep-waiting)**; marker absent **at** the deadline is **also INFRA** — a durability/infra failure (the own-write never became durable), **never RED**. An own-store positive-control timeout is never an isolation-property failure; a genuine mis-route surfaces as RED at the negative-control (cross-store) cell.
- FR-06.3: Negative-control (cross-contamination) queries for a direction are evaluated only **after** that direction's positive control has been satisfied (its marker is present) — so "other store clean" is never read before the write it guards against has had its chance to land.
- FR-06.4: Absent `sqlite3` is **INFRA-fail** (provisioned like node), never a silent empty capture that empty-passes (SR-01).
- FR-06.5: Absent `vol` sidecar, a missing main `unimatrix.db`, or uncopied WAL sidecars that yield a pre-checkpoint false-empty view are **INFRA-fail** (SR-02). A missing main db is INFRA; an absent (already-checkpointed) `-wal`/`-shm` is acceptable only when the main db is present and durable.
- FR-06.6: `warn+continue` on any precondition is forbidden (#4473).

### FR-07: Surface and cell independence
- FR-07.1: The four markers are **mutually non-substring** literals (shared `<run>`-nonce + disjoint per-cell tag); observe markers (`observations` table) and MCP markers (`entries` table) live in distinct tables, so every cell's verdict is independently attributable and no `LIKE '%marker%'` query can cross-match (SR-07).
- FR-07.2: All four writes and both two-store reads run in the **same** container against the **same** A/B slugs/cert/`vol`/sqlite3 primitives.
- FR-07.3: The MCP-write surface uses the streamable-HTTP handshake against both `/v1/A/mcp` and `/v1/B/mcp`; each probe captures and uses its **own** `Mcp-Session-Id`. A's session id is never reused against B's route (a crossed session mis-attributes the test) (FIX 2 / C4).

### FR-08: UDS reference, not re-run
- FR-08.1: The local-UDS isolation guarantee is **referenced** (ADR-006 compile-time guard / `FORBIDDEN_IN_LOCAL`) as proof that no second local route is representable; no UDS behavioral probe is added.
- FR-08.2: No parity-matrix entry and no probe-for-probe parity shape with the UDS leg is introduced (the removed D6, #845; SR-06).

---

## Non-Functional Requirements

### NFR-01: Test-only and cumulative
No production-code change; no fork; no new scaffold. Extend the infra-001
posture-smoke / `cloud-bundle-lib` machinery and reuse its `vol`/cert/token/
sqlite3 primitives. The new isolation gate's assertions are **self-contained** so
an upstream smoke change surfaces as an explicit failure, not a silent skip
(SR-12).

### NFR-02: Distroless-safe volume access
All filesystem inspection of the data volume is via the `vol` busybox sidecar;
**never** `docker exec` into the distroless runtime container.

### NFR-03: Read-dependency provisioning (measurable)
`sqlite3` is provisioned on the host like node and its presence is asserted
(`command -v sqlite3`) before any read; absence is INFRA. Each two-store read
copies `unimatrix.db` + `-wal` + `-shm` for the durable post-barrier view.

### NFR-04: Capability-claim discipline
N3 (#5161) is reported as **`partial`** regardless of outcome — a point-in-time,
bidirectional, two-surface pass; the N5 (#788) standing regression gate is unwired
here. Capability evidence wording is "advances, does not close" (SR-05).

### NFR-05: Determinism, non-collision, and mutual non-substring of markers
The four markers share a single per-run `<run>`-nonce (e.g. PID + timestamp) and a
disjoint per-cell tag — `infra003-obs-a-<run>`, `infra003-obs-b-<run>`,
`infra003-mcp-a-<run>`, `infra003-mcp-b-<run>` — so they are unique per run **and
mutually non-substring**. Uniqueness keeps a stale store artifact from a prior run
from satisfying a positive control or polluting a negative control; mutual
non-substring is **required** because the MCP read is `content LIKE '%<marker>%'`
— if one marker were a substring of another it would false-match a cross-direction
negative control and pass GREEN on a real leak. B's neutral slug `isolation-b`
further guards against collision with a real-project store on the volume.

---

## Acceptance Criteria & Verification Methods

AC-IDs mirror SCOPE.md. The four markers (`A-obs`, `B-obs`, `A-mcp`, `B-mcp`) make
every cell of the 2×2 matrix independently attributable.

### Registration

| AC | Criterion | Verification Method |
|----|-----------|---------------------|
| **AC-01** | Both slugs (A = `arch-research`, B = `isolation-b`) are registered **before a single restart**; after restart all four routes (`/v1/A/observe`, `/v1/B/observe`, `/v1/A/mcp`, `/v1/B/mcp`) respond. Route-liveness alone is **not** a verdict. | Assert `project register` succeeds for both A and B, then one `docker restart`; probe all four routes respond (non-404) before any marked write. INFRA-fail if any route is absent post-restart. The non-404 result is recorded as a precondition, not an isolation pass (SR-11). |

### Observe surface (bidirectional)

| AC | Criterion | Verification Method |
|----|-----------|---------------------|
| **AC-02** | Marked observe writes POSTed to `POST /v1/A/observe` (`A-obs`) **and** `POST /v1/B/observe` (`B-obs`) via the real cert-pinned bearer path each return `204`. | Cert-pinned `curl` POST per direction with the marker in `topic_signal`; assert HTTP `204` for both. |
| **AC-03** *(positive, load-bearing)* | A's store **contains `A-obs`** **and** B's store **contains `B-obs`** — presence assertions via the marker-keyed read (its own barrier), not `du` deltas. | `vol cat` each store's `unimatrix.db`+`-wal`+`-shm`; **bounded retry-until-present**: `sqlite3 -json "SELECT topic_signal FROM observations WHERE topic_signal = '<marker>'"` re-queried until ≥1 row or deadline. ≥1 row for `A-obs` in A and `B-obs` in B passes; not-yet-present before deadline = INFRA (retry); absent at deadline = **INFRA** (durability/infra failure, never RED). An own-store positive timeout is never an isolation-failure RED. |
| **AC-04** *(negative — cross-contamination, both directions)* | A's store does **not** contain `B-obs` **and** B's store does **not** contain `A-obs`. | Same query for `B-obs` against A's store and `A-obs` against B's store; assert 0 rows for both. Never a dir-count/`other_count` heuristic. The B→A cell catches B mis-resolving into A's store. |
| **AC-05** | Each direction's positive control gates its negative control: if a slug's own observe marker is absent from its own store, the test fails RED and does not report the other store's cleanliness as a pass. | Verdict logic: evaluate AC-03 per direction first; on failure emit RED and skip the cross-contamination pass path. Verified by gate-logic inspection + a stub-driven negative case if a stub seam exists. |

### MCP-write surface (bidirectional; same container / slugs / cert / `vol` / sqlite3)

| AC | Criterion | Verification Method |
|----|-----------|---------------------|
| **AC-06** | Marked MCP writes sent to `POST /v1/A/mcp` (`A-mcp`) **and** `POST /v1/B/mcp` (`B-mcp`) (`context_store`/`context_correct` JSON-RPC, tool name in body) via the cert-pinned bearer path each return a success response. | Cert-pinned `curl` JSON-RPC POST per direction with the marker in `content`/`topic`; assert JSON-RPC success (no `error`) for both. |
| **AC-07** *(positive, load-bearing)* | A's store **contains `A-mcp`** **and** B's store **contains `B-mcp`** — via the marker-keyed read (its own barrier). | `vol cat` each store's db+WAL; **bounded retry-until-present**: `sqlite3 -json "SELECT id FROM entries WHERE content LIKE '%<marker>%' OR topic = '<marker>'"` re-queried until ≥1 row or deadline. ≥1 row for `A-mcp` in A and `B-mcp` in B passes; not-yet-present before deadline = INFRA (retry); absent at deadline = **INFRA** (durability/infra failure, never RED). A genuine mis-route surfaces as RED at the cross-store negative control (AC-08), not here. Not a `du` delta nor success-RPC-only (SR-10). Mutual non-substring markers ensure `LIKE` never cross-matches. |
| **AC-08** *(negative — cross-contamination, both directions)* | A's store does **not** contain `B-mcp` **and** B's store does **not** contain `A-mcp`. | Same `entries` query for `B-mcp` against A's store and `A-mcp` against B's store; assert 0 rows for both. |
| **AC-09** | Each direction's MCP positive control gates its negative control (no vacuous pass on a silently-failed MCP write). | Verdict logic: evaluate AC-07 per direction first; RED on failure, skip the pass path (SR-10). |
| **AC-15** *(MCP per-session isolation)* | The streamable-HTTP MCP handshake runs against **both** `/v1/A/mcp` and `/v1/B/mcp`; each probe captures and uses its **own** `Mcp-Session-Id`. A's session id is never reused against B's route (a crossed session mis-attributes the test). INFRA-vs-RED discrimination holds for both probes. | Script inspection + run: each `/v1/{slug}/mcp` handshake response's `Mcp-Session-Id` is captured per slug and sent only on that slug's subsequent requests; assert no cross-slug session reuse. A failed/absent handshake is INFRA (mis-provisioned), a successful handshake whose write mis-lands is RED (FIX 2 / C4). |

### Shared discipline

| AC | Criterion | Verification Method |
|----|-----------|---------------------|
| **AC-10** | Sound per-write durability barrier — **no aggregate `du` barrier**. Writes are strictly sequential per store; each positive-control content query is a **bounded retry-until-present** loop whose marker-keyed read is its own barrier. Marker-not-yet-present before the deadline is **INFRA (retry)**, and absent **at** the deadline is **also INFRA** (durability/infra failure, never RED) — an own-store positive timeout is never an isolation failure; a genuine mis-route is RED at the cross-store negative-control cell. | Inspection: no "A grew AND B grew" aggregate `du` gate; writes sequenced per store; the positive-control query is a deadline-bounded retry loop whose timeout disposition is INFRA. A pre-deadline or at-deadline own-store miss classifies INFRA, pinning the `tokio::spawn` fire-and-forget + `synchronous=NORMAL` race so the positive control does not false-RED (SR-03/#5321). |
| **AC-11** | Reads hard-fail INFRA when `sqlite3` is absent (provisioned like node); `-wal`/`-shm` sidecars are copied with each main db. | `command -v sqlite3` asserted before reads → INFRA on absence; verify WAL sidecars copied for each store (false-empty pre-checkpoint snapshot avoided) (SR-01/SR-02). |
| **AC-12** | Each slug's store is read via its **own per-slug credentials** (over the wire) or an on-disk `vol` read; one slug's credential is never reused to read another's store. | Code/script inspection: each read path uses the `vol` sidecar on that slug's `unimatrix.db` (or that slug's token); assert no cross-credential read (#4950). |
| **AC-13** | Test-only, cumulative on infra-001 — no production change, no fork/scaffold; slug literals are not re-typed ADR-allowlist copies; B = `isolation-b` is a neutral test-scoped literal (R-11) chosen to avoid collision with any real-project store on the volume. | `git diff` shows no `crates/` change; gate lives under `product/test/infra-001/scripts/` (or sibling); A reuses the existing slug constant, B is the single `isolation-b` literal (SR-08). |
| **AC-14** | The local-UDS guarantee is **referenced** (ADR-006 / `FORBIDDEN_IN_LOCAL`), not re-run; no UDS probe, no parity-matrix shape. | Inspection: no UDS write path and no parity-harness entry are added (SR-06). |

---

## Constraints

- **C-01 (test-only, cumulative):** Extend the infra-001 harness; no production-code change, no fork, no new scaffold.
- **C-02 (distroless):** All volume inspection via the `vol` busybox sidecar; never `docker exec`.
- **C-03 (allowlist, SR-08/#4975):** A = `arch-research` (existing), B = `isolation-b`; both valid under ADR-004 `^[a-z0-9][a-z0-9-]{0,62}$` (`seam.rs:86`). The ADR is authoritative, not a restated regex.
- **C-04 (test-scoped slug literal, R-11):** B = `isolation-b`, a neutral name, not a real-project-sounding slug — avoids colliding with a pre-existing store on the test volume.
- **C-05 (single restart, #5079):** Register both slugs before the one restart that applies `[[projects]]`.
- **C-06 (route-liveness ≠ verdict):** A non-404 route is a precondition; the isolation verdict is the content-read matrix only.
- **C-07 (transport identity, #4950):** Read each slug's store via its own per-slug credentials or on-disk via `vol`; never assert one slug's content via another slug's credential.
- **C-08 (sound per-write barrier, no aggregate `du`, #5321):** Strictly sequential per-store writes; the positive-control marker-keyed read is its own barrier via a bounded retry-until-present loop (pre-deadline miss = INFRA/retry; at-deadline own-store miss = **INFRA**, never RED — a genuine mis-route is RED only at the cross-store negative control); copy `-wal`/`-shm`. No "A grew AND B grew" aggregate `du` barrier — it is satisfied by a store's first write and races the second under `tokio::spawn` + `synchronous=NORMAL`.
- **C-13 (MCP per-session isolation, FIX 2):** Each `/v1/{slug}/mcp` probe captures and uses its own `Mcp-Session-Id`; never reuse one slug's session against another's route.
- **C-14 (mutually non-substring markers, SR-07):** The four markers must be mutually non-substring (shared `<run>`-nonce + disjoint per-cell tag), because the MCP read uses `LIKE '%marker%'`.
- **C-09 (sqlite3 hard-fail, SR-01):** Provision sqlite3 like node; absence is INFRA, never an empty-pass.
- **C-10 (four distinct markers, SR-07):** Four distinct marker literals, one per (slug, surface) cell; observe markers in `observations`, MCP markers in `entries`.
- **C-11 (no count heuristics):** No `du`/dir-count/`other_count`; content reads only — the removed D6 count logic is replaced, not extended (ass-084 OoS#2).
- **C-12 (N3 partial, SR-05):** Capability evidence states "advances, does not close N3"; N5 (#788) regression gate is out of scope.

---

## Dependencies

| Dependency | Purpose | Notes |
|-----------|---------|-------|
| Docker | Build + run the shipped multi-slug container | Distroless runtime |
| busybox image | `vol` sidecar for read-only volume inspection | Existing infra-001 idiom |
| `sqlite3` (host) | Content read of each per-slug `unimatrix.db` | **Must be provisioned like node**; absence is INFRA (AC-11) |
| `curl` | Cert-pinned bearer POST to the four routes | Existing idiom; one token, slug in path |
| `node` | JSON shaping of `sqlite3 -json` output / payload assembly | Existing idiom in `cloud-bundle-lib.sh` |
| `unimatrix-server` image | The shipped multi-slug container under test | Exercised as shipped (`MultiProjectRouter`, vnc-038 #5082); not modified |
| infra-001 harness | `docker-http-posture-smoke.sh`, `cloud-bundle-lib.sh` (`vol()`, token/cert retrieval, WAL-aware `vol cat`, `capture_behavioral_topic_signals`, sqlite3 hard-fail) | Cumulatively extended |
| Existing components (referenced, not changed) | `parse_project_key`/allowlist (`http/router/seam.rs:86,199-215`), `MultiProjectRouter::resolve_store` (`project_resolver.rs:204`), `route_observe` (`http/router/handlers.rs`), `observations` write (`unimatrix-store .../observations.rs`), `entries` write (`tools.rs context_store` → `write.rs`), `entries`/`observations` schema (`unimatrix-store/src/db.rs`) | Production seam exercised |
| ADR-006 guard | `FORBIDDEN_IN_LOCAL` / `local_binding_guard_tests.rs` | **Referenced** as proof, not re-run (AC-14) |

---

## NOT in Scope (explicit exclusions)

- **Claiming N3 (#5161) proven/closed.** Point-in-time only; N3 stays `partial`.
- **Wiring the N5 (#788) standing regression gate.**
- **Any UDS behavioral probe.** ADR-006 `FORBIDDEN_IN_LOCAL` is referenced, never re-run.
- **Any parity-matrix entry / probe-for-probe parity with the UDS leg** (the removed D6, #845).
- **Production routing/resolver/store-binding change.** Exercised as shipped.
- **A new pytest suite / H2 orchestrator-level dual-leg capture** (ass-084 H2 deferred); no re-architecture.
- **Reusing or extending the removed D6 `capture_isolation_probe` dir-count/`other_count` logic.** Replaced by the genuine content read.
- **Treating route-liveness (non-404) as the isolation verdict.** It is a precondition only.
- **Over-the-wire read-back of any entry via `context_search`/`context_get`** as a positive control — the on-disk content read is the authoritative isolation proof (over-the-wire read-back is optional corroboration, not the verdict).

---

## Open Questions

- **Q3 (deferred to architecture, leans shell):** Host the gate as a new standalone shell gate mirroring `docker-http-posture-smoke.sh` (reuses `vol`/cert/sqlite3 idioms directly, ass-084 H1) or fold it into the existing smoke script as added gates. Pure structural choice; either keeps assertions self-contained (SR-12). Recommendation: a new sibling shell gate sourcing `cloud-bundle-lib.sh` for `vol`/sqlite3 helpers, keeping infra-001's Gates 1–4 untouched. Architect to confirm.
- **Q5 (deferred to architecture, either works):** Register both slugs before a single restart (preferred for harness simplicity) vs. a second restart — routing read once at boot, so both correct; pick the simpler flow. Recommendation: single restart (C-05).
- **For the architect:** Confirm `context_store` is sufficient for AC-06 or whether `context_correct` (a chained correction) better exercises the `McpAdapter` write path; the spec permits either (tool name in the JSON-RPC body). The load-bearing requirement is the on-disk `entries` content read, independent of which write tool is used.
- **For the architect:** The spec selects `observations.topic_signal` (exact match, the proven `capture_behavioral_topic_signals` idiom) for observe markers; `observations.input` (substring on the raw payload JSON) is a documented fallback if a future payload shape drops `topic_signal`. Confirm `topic_signal` is the stable choice.
- **For the architect:** Confirm the bounded retry-until-present deadline value for the positive-control marker-keyed read (arm64-CI headroom, mirroring the existing ~10s store-grow wait in `docker-http-posture-smoke.sh`). An own-store marker still absent at the deadline classifies **INFRA** (durability/infra failure, never an isolation-property RED); a genuine mis-route is caught as RED only at the cross-store negative control (C-08).
