# ASS-111 S2: Architect Review 2 (Application-layer scale-out)

**Reviewer**: uni-architect
**Date**: 2026-11-26
**Subject**: whether S2's cell + router model (as adopted in `ass-111-findings.md`) scales out horizontally at the application layer. Inputs: `ass-111-s2-findings.md`, `ass-111-s2-architect-review.md` (C1–C15), the S7 decision memo, roadmap and risks, the repo `Dockerfile` and `docker-compose.yml`, and code spot checks (cited inline).
**Human position**: stateful is fine. The application must scale out the way the per-project SQLite data plane already shards. One large vertically scaled box is not acceptable.
**Mode**: research. No ADRs stored, no code changes, no other files edited. Every ADR action below is for the V2 design session.
**Numbering**: the first review already used C14 (`project_id` paths) and C15 (migration under lazy load). New constraints therefore start at **C16**.

## Verdict

**Yes, conditionally.** The data plane, the per-project work units and the per-project learning state are already N-wide in shape. As written, though, S2/S7 only say "design the routing map so a second cell is a configuration change". That is necessary but not sufficient. Seven components are process-global or assume one process today. If they ship that way in Stages 1–3, the first second cell becomes a re-architecture:

1. Router, cell and control plane are composed in-process, with no boundary between them.
2. One shared rayon pool serves query embedding and background ML, and ORT thread counts are unbounded.
3. NLI runs in-cell with an unbounded arena.
4. Placement has no fencing beyond a host-local `flock`.
5. Nothing separates fleet-wide jobs from per-project work.
6. Quotas are keyed per principal, and a principal spans cells.
7. Shared-scope fan-out assumes co-location.

Each is cheap to fix while there is one cell and expensive later. The fixes are C16–C27. The rule that keeps them from rotting is **C16: from Stage 2 on, CI runs a two-cell topology, even while production runs one.**

Rename the model from "tenant-pinned cells" to **"project-placed, tenant-affine cells"**. The physical unit is the project. Tenant affinity is a placement policy, not a schema constraint.

---

## 1. Component inventory: can each run N-wide?

"Day-one" means it must be true in code before α, while there is one cell. "Later" means it is safe to retrofit.

| # | Component | Today (code) | Scope | N-wide as designed? | Day-one requirement | Later |
|---|---|---|---|---|---|---|
| 1 | Project store + HNSW + derived caches | Per slug (`http_provision.rs`). `resolve_store` is the single funnel (#4950) | Project | **Yes**: shards by project | A project is open on **at most one cell**, enforced by an assignment epoch, not only by `flock` (C19). Vectors in SQLite (C1), so a project is a movable file | Rebalancer, automated moves |
| 2 | Per-project learning state (adaptation, usage_dedup, registry, audit) | Per slug (#5164; `http_provision.rs:248`) | Project | Yes | Persisted or classified ephemeral per C2, so a move or crash loses no durable state | — |
| 3 | Session registry, transcript hold, `client_type_map`, MCP sessions | Session registry and transcript hold are per slug (`http_provision.rs:297,308`). MCP sessions use rmcp `LocalSessionManager`, in memory (`http/router.rs:367`) | Cell process | Yes, if loss-tolerant | Router routes by URL `(tenant, project)` and never by session, so routers need no stickiness. Hook/observe traffic for a project reaches the same cell as its MCP traffic. A cell restart or move produces a 404 on a stale `Mcp-Session-Id`, and the Node client re-initializes transparently, with a test (C22) | Stateless MCP revision (R17) removes most of this |
| 4 | Attestation call-id cache (Stage 5) | Not built | Project | Yes, if per project | Keyed per project. Persisted, or drained for its TTL (≤5 min) before a move completes (extends C8) | — |
| 5 | Router | In-process (`MultiProjectRouter`, fixed at boot) | Process | **Not yet** | Router holds **no durable state**. The routing map and auth caches are TTL'd, epoch-stamped caches of the control plane (C9). Router→cell carries one `RoutedRequest {tenant_id, project_id, principal, assignment_epoch}`, even in-process. The cell validates it as if it came from a remote router (C17) | Separate router process(es), mTLS signing, DNS/LB in front |
| 6 | Control plane | Planned: SQLite in-process behind an async trait (C6) | Fleet | Single writer; reads N-wide via caches | **No transaction spans the control plane and a project store.** Cross-plane operations (create project, grant, purge, move) are idempotent multi-step sagas. Cells and routers keep serving established principals for the cache TTL while the control plane is unavailable (P5) (C18) | Postgres (Option C) for HA; read-only followers |
| 7 | Auth / authorization cache | Planned (S1 R4, C9) | Router and cell | Yes | A stated revocation bound (TTL). A grant/principal epoch so invalidation can be pushed later | Push invalidation |
| 8 | Scheduler / tick | `spawn_per_slug_tick`: serial over this process's slugs (`background/tick_loop.rs:85`). Work units already touch only `ctx` + read-only `shared` (`background/job.rs:243-244`) | Cell | **Yes**, if scoped | The scheduler enumerates **only projects assigned to this cell**, from the routing map, never from "all registered projects". No job type iterates projects other than through a per-project work item (C20) | — |
| 9 | Fleet-wide jobs: tenant export/purge, backup verification and retention expiry, quarantine purge, usage rollup, model-epoch re-embed rollout, token/cert rotation | Not separated (several do not exist yet) | Fleet | **No home today** | They run in the control-plane role under a single-runner lease. Their execution fans out to cells as per-project work items (C20) | — |
| 10 | Embed model (query path) | One per process (`SharedTickResources.embed_service`) | Cell | Yes: N cells × ~90 MB | Every stored vector records `model_id` and `dim` (with C1), so placement and moves can check model compatibility (C24) | — |
| 11 | NLI model + ORT arena | One per process. Arena 0.5 → 5.2–9.4 GB [M8]. Off by default (`default_nli_enabled=false`, `w_nli=0.00`), so it is **background-only by default** | Cell | **Scales badly**: N cells × 5–9 GB of peak headroom | Bounded in-cell, or moved behind an `InferenceBackend` seam (C21) | Remote ML worker tier (§2) |
| 12 | Rayon pool + ORT threads | **One** `ml_inference_pool` per process, shared by MCP handlers and background (`main.rs:1047`; #2700, crt-022 ADR-003). ORT sessions set no intra/inter-op thread count (`unimatrix-embed/src/onnx.rs:65`, `cross_encoder.rs:120`). M4's ~15 busy cores against a pool of ≤8 is consistent with ORT's per-host default | Cell | Works N-wide, but **unfair and container-unsafe** | Separate interactive and background inference lanes, with interactive priority. ORT thread counts derived from the cell's CPU budget (cgroup quota), not from host cores (C23) | — |
| 13 | Model catalog | "Cell attribute" (S2) | Cell | Yes, with placement | Placement checks the project's `model_id` against the target cell (or worker) catalog. A model upgrade is a per-project epoch, not per cell (C24) | — |
| 14 | Shared scopes (Stage 6) | Not built. S3 constraint 6 requires co-location | Placement group | **Only if co-located** | A shared store and its granted consumers form one placement group, which moves together. `ReadView` is an async trait returning owned, serializable results, so a remote implementation is possible later (C25) | Cross-cell fan-out over authenticated RPC |
| 15 | Quotas / rate limits | Planned per principal (Stage 3) | Principal (spans cells) | **No**: N cells give N× the quota | Exact enforcement keys on `(principal, project)` at the cell. Tenant- or principal-wide limits are eventually consistent through control-plane aggregation, with stated slack (C26) | Global token bucket if ever needed |
| 16 | Backup (Litestream) | Planned sidecar, one per cell; directory watcher | Cell | Yes | The replica path keys on `project_id` + assignment epoch, so a fenced-out old cell cannot overwrite the new owner's replica (C19) | — |
| 17 | Token table, TLS cert provisioner | File-based on `/data`; self-signed pinned TLS (`Dockerfile`, vnc-034) | Single box | No (V1 posture) | V2 moves tokens to the control plane (G7). Public TLS terminates at the edge/router. Router↔cell uses internal mTLS (C27) | — |
| 18 | `pidfile` `flock` | Exclusive per data directory (`infra/pidfile.rs`) | Host | Fences one host only | Keep it as the local guard. It does not fence a restored or reattached volume (C19) | — |

**What already holds**: the work-unit seam has no cross-context mutable state (`job.rs:243-244`), learning state is per slug, and the field census test (`server_field_census.rs`) already checks per-instance state. Extend that census to classify every `ServiceLayer`/`AppState` field as *project*, *cell-stateless* or *forbidden*. A new process-global mutable field then fails CI (part of C16).

---

## 2. Scale-out units: ranked, design-now vs defer

| Rank | Unit | Recommendation | Design now | Build when |
|---|---|---|---|---|
| 1 | **Cells, N-wide (project placement)** | Adopt. This is the primary horizontal axis. Placement unit = project; default policy = tenant affinity; a shared-scope group is never split | C16–C20, C24–C26. Routing row `project_id → cell_id, epoch, region` | The second cell is added on a capacity or blast-radius trigger (§3), not by redesign |
| 2 | **Router N-wide** | Adopt: a stateless L7 proxy keyed by URL path | C17. No session termination in the router; streaming/SSE pass-through | When router CPU or edge HA needs it. Until then, in-process |
| 3 | **Background ML worker tier** (NLI graph inference, the gated contradiction scan, bulk re-embed for import, migration and model epochs) | Adopt the seam now, build the tier on a trigger. The worker is the same binary in an `ml-worker` role. It is stateless and tenant-agnostic: it receives `(model_id, texts/pairs)` and returns vectors or scores. It never holds a store handle, never persists or logs payloads, and runs behind mTLS. Fair share stays in the cell scheduler, which decides what to send | C21, C23. The `InferenceBackend` trait has local (in-process) and out-of-process implementations. Request/response types are owned and serializable | In Stage 1, if capping the ORT arena does not bound NLI below the cell's memory budget, move NLI to a local worker process. Go network-remote when background ML, not serving, sets cell count (KC1 data) |
| 4 | **Query-path embedding** | **Keep in-cell.** It is on the hot path (one embed per search; per-embed latency ~10–30 ms, #2700). A remote hop adds RTT and a new failure mode to every search, and P5 has no good degrade for "no query vector" | Interactive lane (C23) | Do not split |
| 5 | **Control plane N-wide** | Single writer is sufficient for scale, since it is off the hot path once cached. N-wide only for HA | C18 | Option C (Postgres) on the HA trigger (D3) |
| 6 | **Per-project read replicas** | **Reject for V2.** (a) Searches are not read-only: ~22 of ~55 statements per search are writes (analytics drain, co_access, query_log [M12]), so a replica would need write-forwarding. (b) P7 residency would be duplicated in RAM per replica. (c) The largest project is 49.7 MB live, with p50 search 12 ms [M10/M13]. Per-project load is bounded by its agents (~20, ass-047). A warm standby for HA is a different thing, and D3 owns it | Nothing beyond the single-writer rule (C19) | Only if one project's read load exceeds a cell, which no corpus suggests |
| 7 | Intra-project sharding | Reject (S2) | — | — |

---

## 3. Cell sizing

**Per-process overhead** that every cell duplicates: base RSS ~350 MB [M1], embed model ~90 MB, ORT runtime. That is negligible at 32 GB. **The NLI arena is the only large per-cell duplicate** (5.2–9.4 GB peak [M8]). It sets the minimum cell size for as long as it lives in the cell.

**Cost is linear in cores** on both candidate platforms [P]:
- Hetzner: CCX23 (4 vCPU/16 GB) €73.49, CCX33 (8/32) €117.99, CCX43 (16/64) €234.99. Two CCX33s cost about the same as one CCX43.
- Fly: performance-1x $33 per dedicated vCPU with 2 GB, plus $6 per extra GB.

Cell size is therefore a blast-radius and efficiency choice, not a price choice. Small 4-vCPU cells cost ~25% more per core and run heavy whole-corpus jobs ~4× slower (S2).

| Phase | Cell class | Why |
|---|---|---|
| α/β, NLI in-cell | **1 × 16 vCPU / 64 GB** (CCX43 class) | Headroom for the 9.4 GB NLI peak and the ~15-core contradiction scan until Stage 1 fixes land |
| After Stage 1 (NLI bounded or out of cell, ORT threads pinned, scan gated) | **Standard cell = 8 vCPU / 32 GB** (CCX33 class). Scale by adding cells, never by growing one | Halves blast radius at the same cost. Residency at 20–30 MB per warm heavy project [E] fits hundreds of resident projects |
| GA | N standard cells, plus optional special-purpose cells (deterministic-mode pinned instance class, R14; a different model catalog) | Heterogeneous cells are fine because placement is data |

**Blast-radius policy** (C28 below): cap each cell by tenants or by share of revenue, not only by memory. Without HA (D3), one cell failure is a full outage for every tenant on it. That cap, not CPU, is what forces the second cell first. Proposal: no cell holds more than ~100 tenants or more than one "large" tenant class at GA [E, policy input]. The human sets the number.

---

## 4. Containers and hosting

### What a containerized cell changes

The container does not change the model, but it makes five existing weaknesses load-bearing:

1. **The shutdown budget.** Container stop grace periods are short and configurable (Docker `stop_grace_period`, Fly `kill_timeout`). Today the HNSW dump happens only at graceful shutdown (`infra/shutdown.rs:181-205`). Dumping N resident projects will not fit reliably, so a SIGKILL loses vectors. **C1 (vectors in SQLite) is mandatory for containers**, not only for failover. Shutdown must never be the persistence point.
2. **cgroup memory limit.** An OOM-kill takes every tenant on the cell. The NLI arena is unbounded [M8]. Cap it or move it out (C21) before any memory limit is set.
3. **CPU quota.** `num_cpus` 1.17 sizes the rayon pool from the cgroup quota. ORT sessions set no thread counts and default to per-host cores, which oversubscribes a quota-limited container. ORT thread counts must be explicit (C23).
4. **Models at boot.** The image bakes no model (pattern #4658), and first boot downloads it from HuggingFace into `/shared`, read-write by default (nan-015 ADR-003 #4652). For N cells and for replacing a failed cell, that adds an internet dependency to RTO and a supply-chain surface (embed-model hash, #651). For the V2 cell image, bake the hash-pinned models into the image or pull them from the operator's object store with hash verification (C27).
5. **Posture.** Today's image is the single-tenant OSS posture: self-signed pinned TLS, bearer token file on `/data`, `UNIMATRIX_HTTP_ENABLED=true` (vnc-034 ADR-007 #5135). V2 needs a role-parameterised image (`router | cell | control | ml-worker`, still one binary, P6). Public TLS terminates at the edge, and cells listen on internal mTLS only. Readiness (assignment loaded, models ready) must be separate from liveness: a degraded project must not fail the cell's healthcheck (C27).

Unchanged: one volume per cell is the cell's failure domain. Litestream runs as a sidecar container on the same volume (P6). Distroless with no shell means move, export and import must be binary subcommands, which is already the pattern.

### Platforms for a solo operator

| Rank | Platform | Fit | Main costs and risks |
|---|---|---|---|
| **1** | **Self-managed VMs (Hetzner Cloud CCX) running the container** (compose or systemd), with a managed LB or Caddy at the edge | Cheapest, and cost dominates as cells grow. Local NVMe gives the same single-writer semantics the design assumes. **Hetzner Cloud Volumes** (network block storage, reattachable within a location) offer a host-failure path with RPO ≈ 0: detach, attach to a replacement, flip routing. Fly volumes have no equivalent (needs a PoC: SQLite WAL latency on a network volume against local NVMe) | The operator owns OS patching, firewall, TLS edge, deploy and private networking. ~1 day setup, then routine. €118–235 per cell [P] |
| **2** | **Fly.io Machines + volumes** | Least ops. Anycast edge with automatic TLS, a private WireGuard network for router↔cell, machine restart policy, and `fly-replay: instance=<id>` lets a stateless router hand a request to a specific cell machine. That is project→cell routing without proxying bytes | **~3× Hetzner compute** [D from P]: 8 vCPU/32 GB ≈ $360/mo against CCX33 €118; 16/64 ≈ $720 against CCX43 €235. Volumes are host-pinned NVMe with no replication. On host failure the machine is down until the host returns or you restore elsewhere (Fly docs), which is the same RTO story as local NVMe. Daily snapshots by default (5-day retention) are not a backup. Litestream is still required. `fly-replay` cannot replay bodies over 1 MB, and its SSE behaviour is undocumented: PoC it, or proxy over 6PN instead |
| 3 | Kubernetes StatefulSets (managed or k3s) | Gives stable identity + PVC + rescheduling, but **placement here is an application concern** (the routing map, epochs, moves), so the k8s scheduler buys little. Local PVs pin to nodes, the same as above. Moving a project would need a custom operator | A cluster to run: upgrades, CSI, ingress, cert-manager. **Reject for V2.** Revisit only beyond ~10 cells, and even then optional |
| — | **Cloud Run / stateless serverless** | **Confirmed out for cells.** No durable local disk (the in-memory FS counts against RAM and is lost with the instance). Mounted storage is GCS FUSE or NFS, where SQLite WAL (shared-memory index, POSIX locks) is unsafe. Session affinity is best-effort, so there is no single-writer guarantee, and instances terminate at will. Cold start also defeats P7 residency | **Acceptable later for the stateless `ml-worker` tier** (§2 rank 3), the one component that fits it |

**Recommendation**: build the cell image platform-neutral (C27) and launch on **Hetzner VMs**. Fly is the alternative if the human prefers not to own OS, TLS and networking at ~3× the compute cost. At one cell the difference is ~€120–250/mo, and it grows linearly with cells. Platform choice is escalated (E-A).

---

## 5. Principles and ADRs to amend

| Item | Blocks scale-out? | Action (V2 design session) |
|---|---|---|
| **P6** (one binary; C4 text) | No, but it leaves the in-process composition implicit | **Amend.** Add after the first sentence: *"The roles are router, cell, control plane, ML worker and local single-project. They communicate only through their published interfaces, so running several roles in one process is a deployment choice that configuration can undo, never an architectural coupling."* The rest of the C4 text stands |
| **P7** (resident project; C3 text) | No, but it does not state single residency or where inference runs | **Amend.** Add: *"A project is resident on at most one cell at a time, the cell holding its current assignment. Derived state is never shared or replicated between cells. Query-path inference runs in the serving cell; background inference may run in a separate worker tier."* |
| **vnc-034 ADR-003** (#4950): one funnel; in-process multi-store, not process-per-project | **No.** It rejects a process per *project*. The scale-out unit is a process per *cell* holding many projects, which the ADR's own Nx-model-memory rationale supports | **Correct** (with the first review's amendment): (a) drop "bounded by single-operator N" and "tick-rebuilt"; (b) the resolver resolves only projects assigned to this cell, and an unassigned project returns a new `RouteError::Misdirected` (router re-resolves), not `UnknownProject`; (c) model memory is per cell or per worker pool, never per project |
| vnc-038 ADR-007 (#5086) fixed router map | Yes | Supersede (already listed) |
| crt-056 ADR-005 / ADR-004 (#5168 / #5167) serial tick | Yes, for fairness | Supersede / correct (already listed). Add: the scheduler scope is the cell's assignment set (C20) |
| **crt-023 ADR-001 (#2700)** single NLI `Mutex<Session>` plus pool floor; **crt-022 ADR-003 (#2574)** one pool sized `num_cpus/2` | Yes: fairness and containers | **Supersede** with "interactive and background inference lanes; ORT threads explicit from the CPU budget; NLI behind `InferenceBackend`" (C21, C23) |
| crt-056 C-3/AC-2: one loaded NLI model per process | No | Restate as "per cell or per worker pool" |
| nan-015 ADR-003 (#4652) and pattern #4658: no baked model, first-boot download | Slows cell replacement | **Amend for the V2 cell image only** (C27). The OSS image may keep it |
| vnc-034 ADR-007 (#5135): container HTTP via env | No | Keep for the OSS image. The V2 image uses a role env (C27) |
| vnc-038 ADR-006 (#5087): local UDS path | No | Unchanged: H1 / C5 |

---

## 6. New constraints

They are numbered from C16 because C14 and C15 already exist. They are ranked: C16–C20 matter most, because they are the cheapest now and the most expensive to retrofit.

| # | Constraint | Stage | Blocking for |
|---|---|---|---|
| **C16** | **Two-cell CI topology.** From Stage 2, the integration suite boots two cells, a router and a control plane (separate processes, with a local ML backend allowed). It runs the cross-tenant isolation gate across them and **moves a project between cells** with writes before and after (chain verified, no vector re-embed). Extend the field census to classify every server-state field as project / cell-stateless / forbidden-global | 2 | KC2; α |
| **C17** | **Router↔cell boundary is explicit from day one.** Identity is resolved once at the router into `RoutedRequest {tenant_id, project_id, principal, assignment_epoch}`. The cell validates it as if it came from a remote router, including the in-process composition (signature check stubbed only in-process). The router holds no durable state and routes only by URL path | 2 | α |
| **C18** | **No transaction spans the control plane and a project store.** Cross-plane operations are idempotent sagas with recorded progress. Routers and cells serve established principals from cache for a stated TTL while the control plane is unavailable | 2 | α |
| **C19** | **Single-writer fencing by assignment epoch.** The routing row carries `epoch`. A cell opens a project store only while it holds the current assignment (a lease from the control plane), and closes it on lease loss. A move is drain → snapshot → epoch+1 → open on the destination. The Litestream replica path keys on `project_id/epoch`. `flock` stays as the host-local guard only | 2 (lease trivially held at one cell) | Any move or failover; Litestream |
| **C20** | **Scheduler scope = this cell's assignment set.** Fleet-wide jobs (tenant export/purge, backup expiry and verification, quarantine purge, usage rollup, model-epoch rollout, rotation) run in the control-plane role under a single-runner lease and execute as per-project work items on owning cells. No in-cell job iterates projects outside the work-item seam | 2 / 4 | β |
| C21 | **`InferenceBackend` seam for background ML** (NLI, bulk re-embed, the gated scan), with local and out-of-process implementations. Owned, serializable request/response. The worker never receives a store handle and never persists or logs payloads. Query-path embedding stays in-cell | 1 | R12 closure; KC1 |
| C22 | **MCP and hook session state is cell-local and loss-tolerant.** A cell restart or project move causes client re-initialization, not an error surfaced to agents. The Node client has a re-init test. No session state lives in the router | 3 | α soak |
| C23 | **Inference lanes and thread budget.** Separate interactive (query/store embedding) and background lanes with interactive priority. ORT intra/inter-op threads are set explicitly from the cell's cgroup CPU budget. Measure the fair-share bound (C11) with this in place | 1 / 4 | β fair-share bound |
| C24 | **Model identity is per project.** Stored vectors carry `model_id` + `dim` (with C1). Placement and moves check catalog compatibility. A model upgrade is a per-project epoch rolled out by a fleet job (C20) | 1 | R15 |
| C25 | **Shared-scope placement group.** A shared store and its granted consumers co-locate and move as a unit. `ReadView` is async and returns owned, serializable results, so cross-cell fan-out stays an implementation, not a redesign | 6 (shape fixed in Stage 2 with the qualified-id reservation) | Stage 6 |
| C26 | **Quotas are cell-enforceable.** Exact limits key on `(principal, project)`. Principal- and tenant-wide limits are eventually consistent through control-plane aggregation, with published slack | 3 | α |
| C27 | **V2 cell image.** One binary with a role env. Models baked or pulled hash-verified from the operator store (no first-boot internet dependency). Edge TLS plus internal mTLS (no self-signed client pinning). Readiness separate from liveness. No reliance on graceful shutdown for durability (requires C1) | 3 | First hosted deploy |
| C28 | **Per-cell blast-radius cap** (tenants and tenant class) in placement policy. The human sets the number (E-B) | Pre-GA | GA |

---

## 7. Escalations for human

- **E-A, hosting platform.** Recommended: Hetzner VMs running the container (cheapest; reattachable volumes give an RPO ≈ 0 host-failure path). Alternative: Fly Machines (least ops, ~3× compute). This merges with D3/D16; region is still D16.
- **E-B, blast-radius cap.** The maximum tenants per cell (proposal: ~100, or one large tenant class) and when the second cell becomes mandatory: at β, or at GA together with D3's HA decision.
- **E-C, ML worker timing.** Is the out-of-process NLI worker built in Stage 1 unconditionally (cleaner memory story, recommended if any NLI-on tenant is expected at β), or only if the ORT arena cap fails?

---

Unimatrix entries consulted: #4950 (vnc-034 ADR-003), #2700 (crt-023 ADR-001), #4652 (nan-015 ADR-003), #4658 (Dockerfile pattern), #5135 (vnc-034 ADR-007), #5164, #2492, #2524, #2572, #201, #5836 (briefing). From the first review: #5086, #5087, #5167, #5168, #4129, #5595.
