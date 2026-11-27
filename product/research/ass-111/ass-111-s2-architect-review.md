# ASS-111 S2: Architect Review (Storage, scale & hot path)

**Reviewer**: uni-architect
**Date**: 2026-11-26
**Subject**: `ass-111-s2-findings.md`, checked for consistency with `ass-111-s1-findings.md` and `SCOPE.md`
**Mode**: research. No Unimatrix writes, no ADRs, no code changes. The ADR actions listed below are for the V2 design session.

## Verdict

**APPROVE WITH CONDITIONS.**

The recommendation is sound and S3 may start. The recommendation is Option A: a per-project SQLite + HNSW data plane, a SQLite control plane behind a trait, and tenant-pinned cells behind a router. Nothing in it violates a Hard constraint. It does contain one material factual gap: **entry vectors are not durable in SQLite**. That gap breaks the backup, failover and cell-move story as written. It also has a set of corrections that S3 and S7 must carry forward. Conditions are listed in §6. Items marked **[S3]** bind S3 directly.

---

## 1. Soundness against Hard constraints and binding ADRs

| Constraint | Holds under A? | Notes |
|---|---|---|
| P1 hash chain | Yes | The chain is per store, so per project. A cell move with `VACUUM INTO` keeps the hashes. **Caveat**: restoring from an async replica rolls the chain back by up to the RPO. The chain stays valid but loses entries that were already acknowledged. See the addendum to E4. |
| P2 append-only audit | Yes | `audit_log_no_update/no_delete` triggers are created in every DB (`db.rs:1021-1037`, verified). The control plane needs the same triggers on its own audit table (S1 G11). |
| P3 capability after identity | Yes, with condition C9 | At launch (one process) nothing changes. With multiple cells, the cell's service layer must re-derive capability from an authenticated identity assertion (mTLS plus a router-signed claim). It must never trust bare headers. |
| P8 no secrets in DB | **Conditional (C7)** | S2's control-plane list includes "OAuth clients". It also floats per-project crypto-shred keys for replicas. Neither OAuth client secrets, nor the CI token-exchange signing key (S1 Q3), nor crypto-shred keys may go in the control-plane DB. Store only public metadata, token hashes and device *public* keys. Keep secrets in a KMS or secret file. |
| Tenant isolation incl. learning | Yes, verified | Learning state is per slug. `AdaptationService` is built per slug (`http_provision.rs:234-237`, crt-056 ADR-006 #5164), and so are `usage_dedup`, the registry, the audit log and the vector index. The process-wide pieces are stateless or static: embed and NLI models (crt-056 C-3), the rayon pool, and config (`ConfidenceParams` comes from a preset and is not learned). No global learned state exists today. **Forward risk**: `unimatrix-learn` `TrainingService`/`ModelRegistry` is "wired in future integration step" (`main.rs:1972`). If it is wired per process, it trains on every tenant's signals (C13). Shared-pool contention is a fairness problem, not a learning leak. |
| Graceful degradation (P5) | Yes | A failed vector load boots empty and heals (GH-824, `http_provision.rs:199-217`). An absent NLI worker means fallback. Cold start must degrade, not block, past its bound (C3). |
| Knowledge engine, not orchestrator | Yes | No conflict. |

**Binding ADRs the recommendation overrides.** None is a blocking conflict. Each was scoped to a single operator and has to be superseded or amended in the V2 design session.

| ADR | Conflict | Action at V2 design |
|---|---|---|
| vnc-038 ADR-007 (#5086): "restart applies; no dynamic registry / admin endpoint / in-process reload" | S2 requires a routing map that can change at runtime. S1 G2 also requires it. | Supersede |
| vnc-034 ADR-003 (#4950): resolver seam, "per-slug hot caches … tick-rebuilt" | The tick-rebuilt clause is replaced by residency and build-on-load. S1 G3 recuts the borrow-returning `adapter_for` signature. The single-funnel decision itself still stands. Its "in-process multi-store, not process-per-project" decision also stands: S2's separate NLI worker is per cell, not per project, so it does not conflict. | Amend (correct) |
| crt-056 ADR-005 (#5168): serial per-slug tick loop | S2 builds the Step B scheduler that this ADR deferred but did not preclude. | Supersede |
| crt-056 ADR-004 (#5167): "Step B needs ZERO work-unit changes" | **This claim will not hold.** Dirty-driven enqueue, O(changed) costing, deadlines and priority all need a new `Cadence` input (a dirty or change signal) and per-job cost or deadline metadata. A static `EveryN` cannot express them. | Correct when superseding #5168 |
| vnc-038 ADR-006 (#5087): local STDIO/UDS keeps a direct path-hash binding outside the unified resolver | S2 claims "local N=1 is already the single code path". That is true only for the HTTP daemon with one `[[projects]]` entry. Local UDS/stdio is a separate path by ADR. | Tie to H1 / S1 G16. "Local N=1" means the HTTP daemon with one project, and the UDS path is removed or kept dev-only. |

The recommendation is **supported** by PRODUCT-VISION Decision 3 (per-project knowledge and analytics; a shared analytics DB "is a cross-project observation leakage risk"), quoted by ass-060, and by ass-060 invariants 1–3 and 7.

## 2. Measurements and tagging

The **A-over-B ranking does not depend on any [E] number.** It rests on:
- physical vs logical isolation (structural, and code-verified);
- proven per-slug restore (#5714);
- the size of the SQL port, which I re-counted: 499 `sqlx::query` sites in the store crate and 297 in the server across 27 files [M, confirmed];
- the P7 argument that B still needs tenant-affine caches.

S2 states correctly that cost is a wash. I accept the ranking.

Claims that are estimates but are not tagged, or are tagged too strongly:
1. **"≤ 0.7 s cold start [D]"** is a lower bound, not a derived figure. It leaves out:
   - (a) the effectiveness/confidence snapshot build, which was not measured;
   - (b) schema migration on first open of a lagging DB (independent per-DB migration, ass-060 invariant 2);
   - (c) the HNSW load when the dump is stale or missing. That forces a re-embed of the corpus, tens of seconds on a heavy project (see §4);
   - (d) disk-cold reads: M2 copies had just been written, so they were in the OS page cache.

   Re-tag it as [D, lower bound]. The figure is used in the failover RTO, which is wrong as written (C1).
2. **"A→C ≈ 1 engineer-week"** comes from ass-047 and is [E]. It is used only in the reversibility table, which is acceptable.
3. **M10/M11 (4× search degradation)**: the method is ambiguous. The search ran over "stdio MCP" while the tick ran in the HTTP daemon. If those were two processes, the figure measures CPU contention between processes, not contention on the shared rayon/ORT pool that ADR-005 SR-02 is about. With n=20–30, p90 and max are noise-level. The number is not used for ranking, but it is proposed as the basis for a fair-share bound. Re-measure in-process over HTTP before stating an SLO (C11).
4. **The contradiction scan's role is mischaracterised, and this changes the Q2 priority.** `run_contradiction_scan` writes only to `ContradictionScanCacheHandle` (`background.rs:1062-1116`). Its sole reader is `StatusService::compute_report` (`services/contradiction_cache.rs:35-41`). It is **not ranking state**. So:
   - the "single largest cost line" (83–89 s at about 15 cores per heavy project) feeds `context_status` reporting only;
   - the cheapest fix is to gate it (on demand, or lowest priority with a staleness label), not to make it incremental;
   - the "after fixes" cost row gets better, not worse.

   Remove it from the P7 ranking-state list (C3, C10).
5. M7/M8 are labelled honestly as not yet at a plateau. The "130 GB for 1,000 heavy" figure is a deliberate worst case and is only used to motivate residency. That is acceptable.

## 3. Rejection of shared Postgres; option-set completeness

The rejection is justified. The decisive arguments are B's need for tenant-affine caches anyway (P7), the lost per-tenant restore, and logical isolation across about 800 sites, including the learning tables. The pgvector post-filter argument is secondary. I do not rely on the latency argument (+10–33 ms, [E]).

Options S2 did not include. None of them changes the ranking.

| Option | Assessment |
|---|---|
| **Vectors in SQLite** (BLOB column per entry, pattern #4129; or `sqlite-vec`) | **This should be added as a refinement of A**, not as an alternative. It makes the SQLite file the sole durable per-project artifact, and makes HNSW an index that can be rebuilt without ML inference. That fixes §4. It also enables S2's "incremental contradiction using stored vectors" (no such stored vectors exist today). |
| libSQL / Turso (DB-per-tenant, embedded replicas, native vectors) | A variant of A. sqlx has no libSQL driver, so the driver layer would have to be ported. Turso Cloud adds a required external service, which fails restated P6. Keep it as the fallback if the Litestream PoC fails. The Turso Rust rewrite is not a V2 foundation. |
| Postgres schema-per-tenant | Isolation is better than RLS, and `pg_dump -n` restores one tenant. It still needs the 800-site port, the SQLite→PG dialect change, triggers per schema, and a networked read path. Thousands of schemas × ~25 tables strain the catalog. It loses local N=1. It ranks between A and B. |
| Postgres DB-per-tenant | Worse ops than schema-per-tenant, and gains nothing over A. |
| LanceDB (embedded, object-storage-native vectors) | Affects the vector half only. It adds a second storage engine. A vectors-in-SQLite BLOB column fixes durability more cheaply. Revisit only if HNSW RAM becomes the residency constraint. |
| Durable Objects / D1 | Same shape as A, but the vendor runtime cannot host the Rust binary plus ONNX. Excluded. |
| Process- or microVM-per-tenant | vnc-034 ADR-003 rejects process-per-project because model memory would multiply by N. Process-per-cell, the S2 recommendation, is consistent. |

## 4. Reversibility, and the gap that changes the failover story

I agree with the reversibility table. B is the near-one-way door, and S3 is the most likely route into it by accident.

**The gap: vectors are not durable in the replicated artifact.**
- The schema has no per-entry embedding column. `vector_map` holds only entry id → HNSW data id (`db.rs:794`). The HNSW dump is the only persisted copy of entry vectors.
- The dump is written **only at graceful shutdown** (`infra/shutdown.rs:181-205`).
- Litestream replicates the SQLite file only.

Consequences:
- After a box failure, the HNSW dump on the restored cell is stale since the last clean shutdown, or missing. Recovery means re-embedding (vnc-048 ADR-004 #5696 import path), which is CPU-bound and takes tens of seconds per heavy project.
- **Failover RTO is not "restore + ≤0.7 s".** For a full-cell failure it is the re-embed time of the whole fleet.
- A cell move with `VACUUM INTO` must copy `vector/` consistently with the DB snapshot.
- LRU eviction must dump HNSW on every eviction.

A related latent defect, which S7 should verify: in multi-project HTTP mode the per-slug `AdaptationService` is built fresh and **never `load_state`/`save_state`d**. Only the daemon handle is (`main.rs:975-983`, `shutdown.rs:208-213`, `http_provision.rs:237`). Meanwhile HNSW persists vectors adapted under that lost state (`server.rs:1109`, `services/search.rs:679`). After a restart, or under eviction, the space used to embed queries drifts from the indexed vectors. Residency would turn this from per-restart into per-eviction.

### Additional constraints S3 must honour [S3]

S2's constraint stands: shared scope is its own store, read by fan-out, never ATTACH or cross-DB SQL. It is consistent with S1 (shared space is a project-shaped store with its own chain and audit) and ass-060 (owner store = a distinguished slug). In addition:

1. **Entry IDs are per-store `INTEGER AUTOINCREMENT`, so they collide across stores.** Every reference that crosses a store must be qualified as `(store_id, entry_id)`, using the immutable `project_id` per S1 R7. That covers search results, injection_log, query_log, co_access, graph edges, feature_entries and any cache. Never write an unqualified shared entry id into a reader table.
2. **No cross-store chain links and no synced copies.**
   - Shared writes enter only the shared store's chain and audit. They are attributed with `principal_id` and home `tenant_id` (S1 G9).
   - Corrections and deprecations are visible to readers through live fan-out.
   - If S3 wants copy-in ("fork"), the copy is a new reader-owned entry in the reader's chain with provenance. It is never synced back and never kept in step with the original.
3. **Learning direction is one-way, toward the reader's own store.**
   - Reader queries, votes, co-access and usage must not be written into the shared store's tables (query_log, observations, co_access, confidence inputs), and must not affect its ranking by default. That would be cross-tenant learning influence.
   - Any shared-scope feedback channel must be an explicit, granted, aggregated input owned by the shared store. That is S3's design call.
4. **Derived state stays per store.** Each store's graph, phase-freq, confidence and effectiveness are computed over that store alone. S3 must define a merge or normalisation rule for ranking across stores. Reader-side derived state may refer to shared entries only by qualified id.
5. **Determinism.** A learning-off reader (S4) that fans out to a learning-on shared store gets results that are not deterministic. S3 must either pin a shared snapshot or version per reader, or declare that shared reads are excluded from the deterministic guarantee.
6. **Placement.** A shared store and its consumers must be co-located on one cell and one region; this is automatic at single-cell launch. Otherwise, cross-cell fan-out goes over authenticated internal RPC with a latency bound, and degrades to home-only on timeout (P5). A shared scope never spans regions (S2 Q4).
7. **Fan-out is bounded.** Set a maximum number of grants per reader and a per-store k. Each extra store adds about 33 point reads per search (M12).
8. **Same embedding model.** A reader and every store it fans out to must share the embedding model and dimension, because the model catalog is a cell attribute.
9. **Shared stores are resident and budgeted.** A shared store is hot for all its readers. Its memory and tick cost count against the owning tenant.

## 5. Principle 6 and 7 restatement drafts

Both are acceptable in intent. Amend them as follows.

**P7.** Make three changes:
- (a) Remove "contradiction cache" from ranking state. It is status-only (§2.4).
- (b) Require derived state to be rebuildable from the project's durable store **without ML inference**. This depends on C1.
- (c) Say that cold start degrades rather than blocks.

> **7. In-memory hot path, per resident project.** All ranking-relevant derived state of a resident project (vector index, typed graph, phase frequencies, effectiveness, confidence) is served from memory and never computed from the database at query time. Every piece of it is rebuildable from the project's own durable store without ML inference. A project becomes resident on first request and builds its read-side state before serving, within a stated cold-start bound. Past that bound it serves degraded (Principle 5) rather than failing. It stays current through change-driven rebuilds and is evicted under a per-cell memory budget without losing durable state. Background work is scheduled by change and fair-shared across tenants. One tenant's background work never degrades another tenant's queries beyond a stated, measured bound.

**P6.** The draft says "no required external … service". That is inconsistent with S1, which requires a hosted IdP/AS (S1 Q3, E1). Litestream is also a second binary. Amend:

> **6. One binary, minimal required infrastructure.** The same server binary fills every role (router, cell, local single-project). The data plane needs only that binary, local disk, and an object store for continuous backup. There is no external database, queue or cache service. Identity is delegated to an external authorization server by design, so that signing keys and client secrets stay out of Unimatrix (Principle 8). A backup replicator, if one is used, is a stateless sidecar. A binary-only fallback (snapshot + upload) always exists. Clients remain adapters: the Node edge client, never the binary.

## 6. Conditions

The **blocking** conditions must be resolved before S7 states a failover/RTO, cost or migration claim. The rest carry into V2 design.

| # | Condition | Blocking for |
|---|---|---|
| C1 | **Make the SQLite file the sole durable per-project artifact.** Persist entry embeddings in SQLite (BLOB column, #4129) so HNSW rebuilds without inference. Until then, failover RTO = fleet re-embed time, and S7 must state it that way. | **S7 failover/RTO; Litestream adoption** |
| C2 | Inventory per-project state that lives outside the SQLite file, and classify each item as durable, rebuildable or ephemeral, with eviction behaviour. The inventory covers: HNSW dump, adaptation state (unpersisted in HTTP mode), extraction watermark / neural enhancer / shadow evaluator (`ctx.mutable`), contradiction cache, session registry, transcript hold, attestation call-id cache. | Residency design |
| C3 | Adopt the amended P7 text (§5). | S7 principles |
| C4 | Adopt the amended P6 text (§5). | S7 principles |
| C5 | Correct "local N=1 is already the single code path". It is the HTTP daemon with one project. The local UDS/stdio path is separate by vnc-038 ADR-006 (#5087). Resolve it together with H1 / S1 G16. | S7 H1 |
| C6 | V2 code adds no raw SQL outside the store crate. The control plane sits behind an async, I/O-capable `ControlPlaneStore` trait. This keeps the A→B reversal bounded, since the 297 server-side sites are already the lock-in. | V2 design |
| C7 | The control-plane DB holds no OAuth client secrets, no token-exchange signing key and no crypto-shred keys (P8). Those go to a KMS or secret file. | V2 design |
| C8 | Residency must not evict a project that holds unexpired attestation call-ids (S1 G17, exp ≤ 5 min), or it must persist them. Eviction must never reopen a replay window. | V2 design |
| C9 | Per-request authorization lookups (S1 R4) are cached in memory with a stated revocation bound. In multi-cell mode, the cell trusts only an authenticated router assertion (P3). | V2 design |
| C10 | Re-rank the Q2 fixes. Gate the contradiction scan first, since it is status-only. Then bound S1 enrichment (it writes 0 edges). Then gate NLI to new pairs. Recompute the "after fixes" row on that basis. | S7 cost |
| C11 | Re-measure M10/M11 in-process over HTTP before using them as a fair-share SLO. | V2 acceptance criteria |
| C12 | Litestream checkpoint PoC (`wal_checkpoint(TRUNCATE)` against Litestream's held read transaction), as S2 already lists. Fallback: libSQL/Turso or in-binary snapshot shipping. | Litestream adoption |
| C13 | Every trained model (`unimatrix-learn` TrainingService/ModelRegistry, `unimatrix-adapt`) is per project, or per shared store by explicit grant. Never per cell. | V2 design (isolation) |
| C14 | Storage directories and replica paths key on immutable `project_id` (S1 R7), not the slug. The S2 diagram's `{project}/` must read `{project_id}/`. | V2 design |
| C15 | Define where schema migration runs under lazy load: an eager fleet job, or on load counted inside the cold-start bound. | Residency design |
| [S3] | The nine constraints in §4. | **S3** |

## Escalations for human

S2's four escalations stand. One addendum:

- **E4 addendum (RPO for acknowledged writes).** Async replication (Litestream, 60 s) means a box failure loses up to the RPO of knowledge writes that were already acknowledged. The project chain rolls back with them: still valid, but truncated. NX-DURABILITY (#5595) covers shedding under load, not loss on failure, so this needs an explicit acceptance. Is a non-zero RPO (≤ 60 s) acceptable for V2 launch, or must integrity writes be durable off-box before they are acknowledged? Off-box-before-ack costs write latency or synchronous replication. Merge this with E4 (HA target).

## Out-of-scope discoveries (for S7 / V1 disposition)

1. Per-slug `AdaptationService` state is never persisted in multi-project HTTP mode, while HNSW keeps vectors that were adapted under that state (§4). This is a V1 latent defect. Check its magnitude before relying on adaptation in V2.
2. Per-slug HNSW is dumped only on graceful shutdown. Any crash loses every vector added since the last clean stop until heal re-embeds (V1 and V2).

---

Unimatrix entries consulted: #5083, #5086, #5087 (vnc-038 ADR-004/007/006), #4950 (vnc-034 ADR-003), #5167, #5168, #5164 (crt-056 ADR-004/005/006), #5696 (vnc-048 ADR-004), #4129 (BLOB embedding table pattern), #2153/#2151 (dual-pool), #5567 (NX-STORE), #5595 (NX-DURABILITY), #5836 (personal-cloud goal), #63 (hnsw_rs ADR), #1146 (re-embed after commit).
