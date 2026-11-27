# FINDINGS: ASS-111 S2 — Storage, scale & hot path

**Spike**: ass-111 S2 (campaign ass-111, GH #1050)
**Date**: 2026-11-26
**Approach**: evaluation + measurement (release binary, scratch harness)
**Confidence**: directional overall; **empirical** for every number used to rank options (tagged below)
**Agent**: researcher-ass-111-s2 (uni-spike-researcher)

> **Revision note (post architect review, `ass-111-s2-architect-review.md`, APPROVE WITH CONDITIONS)**
>
> Factual corrections only. No new investigation; I spot-checked the cited code: `infra/shutdown.rs:181-213`, `db.rs:794`, contradiction-cache readers.
> 1. **Vector durability (C1).** Entry vectors live only in the HNSW dump, which is written at graceful shutdown. `vector_map` holds only id→data-id. Litestream would not replicate vectors. I corrected the Q3 failover/RTO and cell-move claims, the Q1 backup row and the Q2 eviction rule. Vectors-in-SQLite is added as a required refinement of Option A.
> 2. **Contradiction scan is status-only (C10).** It feeds `context_status`, not ranking. I removed it from ranking state, re-ranked the Q2 fixes (gate the contradiction scan → bound S1 → NLI on new pairs only), recomputed the "after fixes" cost row, and corrected Out-of-Scope #1.
> 3. **Cold start re-tagged.** It is now [D, lower bound] with the omissions listed. M10/M11 now carry a methodology caveat.
> 4. **"Local N=1 is the single code path" corrected (C5).** That holds only for the HTTP daemon with one project. Local UDS/stdio is a separate path by vnc-038 ADR-006.
> 5. **P6/P7.** I adopted the architect's amended text.
> 6. **Escalations and discoveries.** Added the E4 addendum (RPO for acknowledged writes) and two architect discoveries: per-slug AdaptationService state is never persisted, and HNSW is dumped only on graceful shutdown. The Recommendations Summary is updated to match.

**Number tags** (so the architect can check what rests on what):
- **[M]** measured in this spike · **[D]** arithmetic derived only from [M] numbers · **[P]** published price, source cited · **[E]** estimate — *not used for ranking*

---

## 0. Measurement basis

**Environment**: 16 vCPU / 64 GB RAM / NVMe-backed overlay, Linux 6.12. Binary: `target/release/unimatrix` built from `main` 2026-11-26 03:02. Multi-project HTTP daemon (`serve --foreground`, `UNIMATRIX_HTTP_ENABLED=true`, `[[projects]]` routing) — the same code path a SaaS cell would run. Embed model MiniLM-L6 (90 MB ONNX), NLI MiniLM2-L6-H768 qint8 (83 MB ONNX), both shared per process (crt-056 C-3) [M].

**Tenant corpora used**
- **Heavy** = a VACUUMed copy of the live Unimatrix-on-Unimatrix store, the largest real corpus in existence: 5,933 entries (2,422 active / 1,006 deprecated / 2,505 quarantined), 24,493 tags, 42,525 co_access rows, 6,452 observations, 5,580 audit rows, 1,713 graph edges, 2,440-point HNSW. 48.7 MB DB + 5.2 MB HNSW dump [M].
- **Light** = a 192-entry test fixture (avg content 116 chars — shorter than real entries, so a *lower bound*) [M].
- No other real project corpus exists on this host (3,432 local stores; all others are test fixtures, median 1 entry) [M]. Med Search's corpus was not available. **Tenant-mix assumptions in §Q1 cost model are therefore explicit inputs, not data.**

**Harness** (throwaway, scratchpad only, nothing in repo): `bootn.sh`/`reboot.sh` (register N slugs, copy template stores, boot, time to `HTTP transport active`, RSS/fds/threads), `multi.sh` + `sample.sh` (5 s tick, debug log, 2 s RSS/CPU sampling), `timeline.py` (per-slug job durations from log timestamps), `mcp_search.py` (stdio MCP `context_search` latency + `sqlx::query` statement counts). Located under `/var/tmp/claude-1000/-workspaces-unimatrix/19789040-5002-4a22-aa04-29e1732b9913/scratchpad/bin/`.

### Measured results

| # | Measurement | Result |
|---|---|---|
| M1 | Boot, empty projects N=1/10/100 (first boot) | 0.21 s / 0.62 s / 2.46 s; RSS 348 / 360 / 554 MB → **~2 MB, ~20 ms, ~4.5 fds, 2 OS threads per idle project** |
| M2 | Boot, heavy projects N=1/10 | first boot 0.21 s / 1.03 s; re-boot 0.11 s / 0.42 s; RSS 362 / 457 MB → **~10.5 MB and 35–90 ms per heavy project for store open + HNSW load** |
| M3 | Hot path at boot | **Not built.** Typed graph, phase-freq, effectiveness, contradiction cache are first built by the first tick (default 900 s; t=0 tick skipped) |
| M4 | Heavy project, per tick-job wall time (5 tenants × 2 passes, consistent ±5%) | maintenance+heal 0.1–1.1 s · typed-graph rebuild **0.26–0.42 s** (3,428 nodes) · phase-freq ~0.01 s · extraction 0.04–0.15 s · NLI graph-inference **4.7–4.9 s** (100 pairs) · S1 graph enrichment **27.7–29.2 s, single core, 0 edges written** · contradiction scan (every 4th tick *and* tick 0) **83.4–89.4 s wall at ~15 cores busy** |
| M5 | Heavy project, full pass | **~30 s** normal tick; **~117 s** contradiction tick. 5 heavy projects = **578 s** for one first pass |
| M6 | Light project (192 short entries) | contradiction scan ~1.0 s; NLI 0.5–0.6 s; everything else <0.2 s → **~1.7 s** first pass, **~0.6 s** normal pass |
| M7 | Process RSS, NLI off, heavy ×5 | 400 MB → 506 → 655 → 791 → 1,065 MB over 2 passes (not plateaued at 11 min) → **~110–160 MB per warm heavy project**, ~6 OS threads each (sqlx = 1 thread per SQLite connection; `cache_size=-16384` = 16 MiB page cache *per connection*, up to 9 connections/project) |
| M8 | Process RSS, NLI on, heavy ×1 | 500 MB → **5.2 GB** after first NLI pass → **9.4 GB** after second, then flat. Same run with NLI off stays ~500 MB → ONNX-runtime arena growth; **process-wide, not per project** |
| M9 | Live production daemon (1 real project, 3 d uptime, NLI on, Sep build) | RSS **1.33 GB** |
| M10 | `context_search` k=5, heavy corpus, stdio, idle | p50 **12 ms**, p90 27 ms, max 31 ms (n=30) |
| M11 | Same, while a tick contradiction scan runs | p50 **50 ms**, p90 95 ms, max 178 ms (n=20) — about 4× slower at the median |

*M10/M11 methodology caveat (post-review)*: both ran in one `serve --stdio` process with `UNIMATRIX_TICK_INTERVAL_SECS=5`. That process ran its own legacy single-store tick (`spawn_background_tick`), so the contention is in-process. It is **not** the per-slug HTTP daemon path (`spawn_per_slug_tick`). With n=20–30, only the p50 shift is meaningful; p90 and max are noise. Re-measure in-process over HTTP before using this as a fair-share SLO (architect C11). Not used for ranking.
| M12 | SQL statements per `context_search` (k=5) | **~55** (≈33 synchronous reads: per-candidate entry fetch, tag fetch, co_access reads, existence checks; ≈22 writes, mostly via analytics drain) |
| M13 | On-disk, heavy production store | 150.9 MB file, **49.7 MB live, 101 MB free pages** (never vacuumed; every graceful shutdown logs `skipping compact: outstanding Store references`) |

---

## Findings

### Q: "Per-tenant SQLite (+ per-tenant vector index) vs shared Postgres (+ pgvector or equivalent) vs hybrid: which meets isolation, hash-chain/audit guarantees (DDL triggers), backup/restore per tenant, and cost at 10 / 100 / 1,000 tenants?"

**Answer**: Keep **per-project SQLite + per-project HNSW as the data plane** and add a small **control plane** (tenants, accounts, OAuth clients, token hashes, project registry, project→cell routing, quotas). Start the control plane in SQLite inside the same binary, behind a trait so a move to Postgres later is cheap. Ranked:

| Rank | Option | Isolation | Hash chain / audit | Per-tenant backup/restore | Code cost | Verdict |
|---|---|---|---|---|---|---|
| **1** | **A — per-project SQLite data plane + SQLite control plane** (upgrade path → C), **refined with vectors-in-SQLite** | Physical. A cross-project query cannot be written; `resolve_store` stays the only funnel (C3/C4 proven, ass-060 invariants 1–3, 7 hold unchanged) | Unchanged code: per-project chain, `audit_log_no_update/no_delete` triggers created per DB (`db.rs:1024-1037`, `migration.rs:1249-1263`) | File-level export/import already proven per slug: BACKUP-RESTORE (#5714, vnc-048 AC-09/AC-10, hash-validated, all tables; vectors are re-embedded on import). Continuous replication (Litestream) covers **the SQLite file only**. Entry vectors exist only in the HNSW dump, written at graceful shutdown (`infra/shutdown.rs:181-205`; `vector_map` is id→data-id only, `db.rs:794`). The refinement fixes this | Control plane, plus an embedding BLOB column (pattern #4129). No data-plane rewrite | **Recommended** |
| 2 | **C — per-project SQLite data plane + managed Postgres control plane** | As A | As A | As A | A + a Postgres client for ~5 control tables | **Use when** the control plane needs multi-node HA (≥ ~3 cells or an HA requirement). A→C costs about one engineer-week (ass-047 Q3 estimate for control-plane-only Postgres) |
| 3 | **B — shared Postgres + pgvector, tenant/project columns + RLS** | Logical. Every statement has to be tenant-scoped. RLS needs `FORCE ROW LEVEL SECURITY`, a non-owner app role and `SET LOCAL` per transaction. Table owners and `BYPASSRLS` roles skip it ([PG docs](https://www.postgresql.org/docs/current/ddl-rowsecurity.html)). One missed predicate in about 800 SQL call sites is a cross-tenant leak, and learning tables (co_access, query_log, observations) leak learning as well as data | Reachable, but every guarantee has to be proven again: BEFORE UPDATE/DELETE row triggers, a *statement-level* BEFORE TRUNCATE trigger (row triggers don't fire on TRUNCATE, [PG docs](https://www.postgresql.org/docs/current/sql-createtrigger.html)), and REVOKE UPDATE/DELETE/TRUNCATE. The chain needs to be scoped by (tenant, project) | PITR restores the whole cluster. Restoring one tenant means a logical extract by tenant_id, so the proven vnc-048 path is lost | Port about 800 raw SQL sites (499 in store, ~300 in server across 27 files) [M]. Re-prove NX-STORE "backend-swappable", which is unproven today (#5567) | **Reject for V2** |

**Why B gains nothing even before cost**
1. **Principle 7 keeps per-tenant state in RAM anyway.** HNSW, typed graph and phase-freq are per-project in-memory structures. pgvector would only provide durability, not query service. A "stateless API over Postgres" still needs tenant-affine caches, so B's main benefit (stateless horizontal scale) disappears.
2. **pgvector with HNSW filters *after* the index scan.** A `WHERE tenant_id=…` on a shared index returns about `ef_search × selectivity` rows; the README gives about 4 rows for a 10% filter at the default ef_search=40. Iterative scans (0.8.0+) help, but the README's own advice for multi-tenant is "consider partitioning" or separate tables ([pgvector README](https://github.com/pgvector/pgvector)). That means thousands of per-tenant partitions, which is per-tenant objects again with extra steps.
3. **The read path is chatty.** About 33 synchronous point reads per search [M12] cost nothing against a local embedded file (whole search p50 12 ms [M10]). Over a network at 0.3–1 ms RTT that adds roughly **+10–33 ms per search** [E, not used for ranking; no Postgres instance was measured].
4. **H1 coupling.** A shared Postgres forces a second backend for any N=1/local shape, so it decides H1 by side effect (see Escalations).

**Cost model at 10 / 100 / 1,000 tenants**

*Inputs.* Cell = Hetzner CCX43 (16 dedicated vCPU, 64 GB, 360 GB NVMe), €234.99/mo; CCX33 (8 vCPU / 32 GB) €117.99; CCX23 (4 vCPU / 16 GB) €73.49 (HEL1, excl. VAT) [P: [hetzner.com/cloud/general-purpose](https://www.hetzner.com/cloud/general-purpose)]. My test host is the same class as a CCX43. R2 storage $0.015/GB-mo, Class A ops $4.50/M (1M free), egress free [P: [developers.cloudflare.com/r2/pricing](https://developers.cloudflare.com/r2/pricing/)]. Neon Launch $0.106/CU-h (1 CU ≈ 4 GB), storage $0.35/GB-mo [P: [neon.com/pricing](https://neon.com/pricing)].

*Tenant-mix assumption (explicit, not data):* 10% heavy (M4/M5 profile), 90% light-realistic (~500 entries × ~1,200 chars). Light-realistic tick cost is **[E]**: contradiction scan scaled linearly from heavy by active-entry count (≈ 87 s × 500/2,422 ≈ 18 s), other jobs ≈ 3 s. The *measured* light fixture (M6) is a lower bound.

*The binding constraint is the serial per-slug tick* (`background/tick_loop.rs`: one slug after another; a pass longer than the interval puts tokio's interval into burst mode, so ticking becomes continuous). Average tick wall time per project: heavy (3×30 + 117)/4 ≈ **52 s [D]**; light fixture ≈ 0.9 s [D]; light-realistic ≈ 7 s [E]. Mix ≈ 0.1×52 + 0.9×7 ≈ **11.5 s per project per 900 s interval [E]**. While contradiction scans run they use about 15 cores [M4], so one serial loop effectively takes one 16-core box.

| | 10 tenants | 100 tenants | 1,000 tenants |
|---|---|---|---|
| **Current algorithms** (serial tick, full-corpus jobs every tick) | ~115 s/pass → 1 cell. CCX43 €235 (CCX23 would make heavy scans ~4× slower) | ~1,150 s/pass > 900 s → **tick cannot keep its interval**. 2 cells ≈ €470 | ~11,500 s/pass → **~13 cells ≈ €3,050/mo** [E] |
| **Fix 1 only: gate the contradiction scan** (status-only → on demand or lowest priority; heavy pass 52 → 30 s avg [D]; mix ≈ 0.1×30 + 0.9×~3 ≈ 5.7 s/project/tick [E] → ~158 projects per serial loop) | 1 × CCX23 €73 | 1 × CCX43 €235 | ~7 cells ≈ €1,650 [E] |
| **Fixes 1–3** (+ bound S1, which is 28 of the remaining 30 s and writes 0 edges [M4]; + NLI only on new pairs; heavy ≈ 2–7 s/tick [E]) plus change-driven gating (assume 20% of tenants active/hour) | 1 × CCX23 €73 | 1 × CCX33–43 €118–235 | 1–2 × CCX43 €235–470 [E] |
| Option A storage + continuous backup (Litestream→R2, 60 s sync) | ≈ $0 (free tier) | ≈ $0–4 | live ≈ 100×50 MB + 900×5 MB ≈ 9.5 GB [D]; ×3 with history ≈ $0.45 storage; PUTs: 200 dirty tenants ≈ 8.8M/mo ≈ **$35**; 1,000 always-dirty ≈ 43.8M ≈ **$193** [D from P] |
| Option B delta (DB only; compute unchanged) | Neon 2 CU always-on ≈ $155/mo + storage | ≈ $160 | 4 CU ≈ $310 + storage ≈ $315; or self-host on a CCX23 (€73) + backup/upgrade ops [D from P] |

**Reading the model**: the storage choice moves cost by tens to a few hundred dollars a month. **Compute for background ML moves it by thousands**, and it is identical under A and B. The largest single line, the contradiction scan, is *status-only*: it feeds `context_status`, not ranking (architect review §2.4). Gating it is the cheapest fix and removes ~40% of heavy-project tick time without touching retrieval. The cost ranking among A/B/C is therefore a wash on infrastructure dollars. A wins on isolation, proven per-tenant restore and zero migration. For the operator, the cost problem is the tick algorithms, not the database (Q2).

**Recommendation**: Use **Option A** — per-project SQLite + per-project HNSW data plane plus a SQLite control plane in the same binary behind an async `ControlPlaneStore` trait. Add V2 SQL only inside the store crate (architect C6). **Refine A by persisting entry embeddings in SQLite** (BLOB column, pattern #4129). That makes the SQLite file the sole durable per-project artifact and makes HNSW rebuildable without ML inference (architect C1). This is a precondition for continuous backup, failover and eviction. Switch the control plane to Postgres (Option C) only on an HA trigger. Do not adopt shared Postgres for V2. Keep the tenant→project→store hierarchy so that **project, not tenant, stays the physical store** (tenant is a control-plane grouping). This preserves ass-060 invariants 1–3 and 7, C3/C4, and the 1-client:1-project integrity boundary untouched.

---

### Q: "What replaces or restates Principle 7 (in-memory hot path rebuilt by tick) when N tenants share a fleet — per-tenant caches, eviction, lazy load? Same for the background tick and ML models (ONNX/NLI) per tenant, same for graph in memory updates."

**Answer**: Principle 7's intent survives ("ranking-relevant derived state is served from memory, never computed from the DB at query time"). Its mechanism does not survive N tenants, for four measured reasons:
1. **All projects open eagerly at boot and stay resident.** `MultiProjectRouter` is "a fixed map built once at boot, no runtime mutation (register/delete restart the server)" (`http/router/project_resolver.rs`). A warm heavy project holds ~110–160 MB [M7]. 1,000 resident heavy projects would need ≈130 GB [D].
2. **The hot path is built by the tick, not on load** [M3]. A lazily loaded project would serve with no graph until the next tick reached it.
3. **The tick runs every job over every project every interval, whether or not anything changed.** S1 re-runs a quadratic tag self-join for ~28 s and writes 0 edges [M4]. NLI re-scores 100 pairs every tick [M4/M6]. The contradiction scan re-embeds the whole active corpus every 4th tick and at tick 0 [M4] (ass-104 I-15, now measured). Its output is status-only: it writes `ContradictionScanCacheHandle`, read only by `StatusService::compute_report`, so it is not ranking state.
4. **Background ML slows queries in the same process.** Search p50 goes 12 → 50 ms while one scan runs [M10/M11]. That was measured on the stdio path only, with small n; see the caveat in §0. In SaaS that becomes cross-tenant interference, a fairness problem rather than a learning leak.

**Restated mechanism (what replaces it)**

| Concern | Rule |
|---|---|
| **Residency** | A project is *resident* (store pool open, HNSW + derived caches in RAM) or *cold* (files on disk only). Load is lazy on first request. Eviction is LRU, driven by a per-cell memory budget plus an idle timeout (ass-047 proposed 30 min). The routing map becomes runtime-mutable (`ArcSwap`/`RwLock`), so register/attach no longer need a restart. This supersedes vnc-038 ADR-007 (#5086). **Eviction must not lose durable state.** Until vectors are in SQLite, every eviction must dump HNSW, and per-slug AdaptationService state must be persisted (today it never is in HTTP mode). An inventory of out-of-file per-project state is still needed (architect C2) |
| **Build-on-load** | Loading a project builds its read-side caches before it serves: typed graph (0.26–0.42 s heavy [M4]), phase-freq (~0.01 s), effectiveness/confidence snapshot. Cold-start cost for the largest known corpus ≈ **0.04–0.09 s open+HNSW [M2] + ~0.3–0.4 s graph/phase-freq [M4] → 0.7 s is [D, lower bound]**. It leaves out four things: (a) the effectiveness/confidence build (not measured); (b) schema migration on first open of a lagging DB; (c) HNSW re-embed when the dump is stale or missing, tens of seconds for a heavy project until vectors are in SQLite; (d) disk-cold reads (the M2 copies were in the OS page cache). Past the bound, a project serves degraded (P5) rather than blocking. Write-side maintenance (heal, GC) is split out of this path and runs only in the scheduler |
| **Per-project memory budget** | Make SQLite page cache × pool size a tenant-class setting. Today it is 16 MiB per connection × up to 9 connections per project [M7]. For SaaS, start at 2 read connections and 2–4 MiB cache (→ ~20–30 MB per warm heavy project [E]), with 1 OS thread per open connection (sqlx) |
| **Tick → change-driven scheduler** | Replace the serial all-projects loop with a cell-level work queue. This supersedes crt-056 ADR-005 (#5168) and corrects ADR-004 (#5167): its work-unit `Cadence` needs a change signal and cost/deadline metadata. (a) A project is enqueued only when **dirty** (writes since its last build) or its cadence is due *and* it is resident. Idle projects cost 0. (b) Fixes in priority order: **1. gate the contradiction scan** (status-only: run on demand or at lowest priority with a staleness label); **2. bound or make S1 incremental** (28 s/tick, 0 edges); **3. NLI only on new candidate pairs**. Incremental contradiction over stored vectors becomes possible only after vectors-in-SQLite. (c) A **global ML budget with per-tenant fair share** (weighted round-robin) and priority for interactive query embedding over background ML, so no tenant's scan can hold another's query for 4× [M11]. (d) Per-job deadline and liveness (ass-104 CC-1 TICK-LIVENESS). (e) Learning-off/deterministic projects (S4) are skipped by every learning job |
| **ML models** | One copy per cell process, never per tenant (already true: crt-056 C-3/AC-2). The model catalog is a cell attribute (≤2–3 models). A tenant that needs a different model is pinned to a cell that has it. **Bound the NLI arena**: one NLI pass took the process from 0.5 to 5.2–9.4 GB [M8]. Cap the ORT arena and batch size, or run NLI in a separate ML worker process so its peak can't OOM the cell's tenants. Per-project `nli_enabled` overlay already exists (slug config) |
| **Graph in-memory updates** | Per project. Keep the full rebuild (0.26–0.42 s at 3.4K nodes [M4]) as the correctness oracle. Trigger it by a dirty flag with debounce (ass-104 CC-7 "fix the window, then trigger the rebuild"), never on a fixed all-tenant cadence. Incremental patching stays behind a measured trigger (ass-104). Swap snapshots with `Arc` swaps, not clone-per-search (ass-047 OSD-5) |

**Principle 7 restatement draft** (for S7; not an edit to PRODUCT-VISION.md). **This is the architect's amended text (review §5, C3), adopted.** It removes the status-only contradiction cache, requires derived state to be rebuildable without inference, and says cold start degrades rather than blocks.
> **7. In-memory hot path, per resident project.** All ranking-relevant derived state of a resident project (vector index, typed graph, phase frequencies, effectiveness, confidence) is served from memory and never computed from the database at query time. Every piece of it is rebuildable from the project's own durable store without ML inference. A project becomes resident on first request and builds its read-side state before serving, within a stated cold-start bound. Past that bound it serves degraded (Principle 5) rather than failing. It stays current through change-driven rebuilds and is evicted under a per-cell memory budget without losing durable state. Background work is scheduled by change and fair-shared across tenants. One tenant's background work never degrades another tenant's queries beyond a stated, measured bound.

Note: the current text "Never read from the database at query time" is already not literally true. Search issues ~33 synchronous point reads per query [M12] (entries/tags). The restatement scopes the rule to *derived* state, which is what the code actually guarantees, and makes embedded local storage part of the principle's intent.

**Recommendation**: Build these **before** onboarding external tenants: vectors-in-SQLite; residency (lazy load + build-on-load + LRU eviction that loses no durable state + runtime-mutable routing map); and the change-driven, fair-share scheduler. They are prerequisites, not optimizations: the serial tick stops keeping its interval at ≈ 75–80 mixed tenants [E] or ≈ 17 heavy ones [D]. In the same work, apply the fixes in this order: (1) gate the status-only contradiction scan; (2) bound S1; (3) NLI on new pairs only. They are the dominant cost line in every column of the Q1 table.

---

### Q: "What is the horizontal-scale model (stateless API + tenant-pinned workers? sharding?) and its operational burden for a solo operator?"

**Answer**: Use **tenant-pinned cells behind a thin router**, with no intra-project sharding and no Kubernetes.

```
             ┌─────────────── control plane (SQLite → Postgres on HA trigger) ─┐
 client ──►  │ router: TLS, auth→(tenant,project) per S1, lookup project→cell │
             └──────────────┬──────────────────────────────┬──────────────────┘
                            ▼                              ▼
                 cell A (same binary)            cell B (same binary)
                 local NVMe: {project_id}/unimatrix.db (+ vectors)   models ×1
                 residency LRU + fair-share scheduler           Litestream → R2
```

- **A stateless API is not possible without giving up Principle 7.** Every request would cold-load (≥0.7 s [D, lower bound]) or would need a shared cache tier, which is new infrastructure. Pinning a project to a cell keeps its caches warm and its files local.
- **No sharding inside a project is needed.** The largest real project is 49.7 MB live [M13], and ass-047's per-project write ceiling (~20 concurrent agents) is a per-project limit, not a fleet limit.
- **Scale-out** = add a cell and move projects: drain writes, snapshot (`VACUUM INTO`/export), copy, flip the routing row. A heavy project is ~54 MB on disk [M], so a move takes seconds. **Until vectors are in SQLite**, the move must also take a fresh HNSW dump consistent with the DB snapshot (the dump is written only at graceful shutdown, `infra/shutdown.rs:181-205`), plus the per-slug adaptation state. Otherwise the destination re-embeds the corpus.
- **Failover** = restore the project's Litestream replica onto another cell and flip routing. RPO = sync interval: acknowledged writes in that window are lost, and the project chain rolls back with them (see E4 addendum). RTO depends on vector durability:
  - **Today**: Litestream replicates the SQLite file only. Entry vectors exist only in the HNSW dump from the last *graceful* shutdown, which is stale or missing after a box failure. RTO for a full-cell failure is therefore the **fleet re-embed time** (vnc-048 ADR-004 import path). That is CPU-bound and tens of seconds per heavy project; ~85 s at ~15 cores to re-embed 2,422 active entries [M4, contradiction-scan analogue]. Not "restore + 0.7 s".
  - **With vectors-in-SQLite**: RTO ≈ restore + HNSW rebuild from stored vectors (no inference) + cache build ([D, lower bound] ≥0.7 s per project). Litestream v0.5 replicates whole directories, and its directory watcher picks up new DBs without a restart; Fly's write-up says "hundreds or thousands of databases" are practical under LTX ([litestream.io/guides/directory](https://litestream.io/guides/directory/), [directory-watcher](https://litestream.io/guides/directory-watcher/), [fly.io/blog/litestream-v050-is-here](https://fly.io/blog/litestream-v050-is-here.md)).
- **The router is the same binary in a router role**, or a reverse proxy plus a routing map. Cell trust of router-asserted identity (mTLS router↔cell) is an S1 item.

**Operational burden, solo operator**

| Stage | Topology | Ops added vs today's container |
|---|---|---|
| 10–100 tenants | **1 cell, router + control plane in the same process** (today's daemon + residency + scheduler + control plane) | Litestream sidecar + one R2 bucket; metrics endpoint (ass-047 Q7, still missing); alerting on tick liveness. No HA: one box failure = full outage. RTO = restore + fleet re-embed until vectors are in SQLite (then restore + rebuild); RPO = Litestream sync interval |
| 100–1,000 | 1–2 cells after Q2 fixes (≈13 without them [E]); optional warm-standby cell | Routing-map moves, per-cell capacity dashboard. Consider Option C control plane once a standby exists |
| >1,000 / HA requirement | N cells, Postgres control plane (C), router pair | Postgres ops (managed), cell rebalancing tool |

**Recommendation**: Run **one cell, router and control plane in one process** for V2 launch. Design the routing map (`project → cell, region`) from day one so a second cell is a configuration change, not a re-architecture. Defer HA until an explicit requirement (escalated). Do not use Kubernetes, Citus or any shared-cache tier.

---

### Q: "What does per-tenant data export/offboarding and residency require?"

**Answer**

**Export (portability)** — most of it exists.
- A per-project lossless, hash-validated export/import across all tables is **proven** (BACKUP-RESTORE #5714; `unimatrix export --slug`, vnc-048 AC-09/AC-10).
- V2 adds: (1) a **tenant-level self-serve export**: an async job over all of a tenant's projects producing the JSONL bundles, a `chain_verify` report per project, and a signed download URL. (2) Optionally the raw SQLite file. Under Option A that file is itself a portable, verifiable artifact the tenant can open with the same binary at N=1, which is the strongest exit story available. Under Option B it doesn't exist.
- Exported chain heads should be given to the tenant (see out-of-scope: chain anchoring).

**Offboarding (erasure)**
- `project delete --purge --confirm <slug>` exists for one project.
- V2 needs a **tenant purge** that covers: all project dirs; control-plane rows (accounts, OAuth clients, token hashes, routing); **backup replicas** (R2/Litestream history); and logs carrying tenant span fields (ass-047 Q7).
- Erasure is only complete when backups expire. The operator must therefore set and publish a backup-retention window (e.g. 30 days) as the erasure SLA, or encrypt each project's replica with a per-project key and crypto-shred it on purge.
- Whole-project purge is consistent with Principle 2: the integrity domain ends. Record the purge itself in the control-plane audit.
- **Per-record erasure inside a live project conflicts with Principles 1/2** (append-only audit, immutable chain). Policy: no personal data in knowledge or audit fields beyond pseudonymous agent/session ids. Public-chat traffic (Med Search) must not be logged into `query_log`/observations (S4's read-mode/no-learning rule covers this). This needs a human/legal decision (escalated).

**Residency** — under Option A this is a placement decision.
- Put the project's cell in region X and its backups in a jurisdiction-restricted bucket. R2 offers `eu` and `us` jurisdictions guaranteeing *storage* location; the docs don't claim processing location ([R2 data location](https://developers.cloudflare.com/r2/reference/data-location/)).
- The routing map carries `region`.
- The control plane holds personal data (account email, OAuth subject), so strict residency needs either a control plane per region or a deliberately minimal global one.
- **A shared scope cannot span regions** without moving data across the residency boundary. Feed this to S3.
- Under Option B, residency means one Postgres cluster per region from the start.

**Recommendation**: Ship V2 with (1) tenant-level async export (JSONL + chain report + optional SQLite file), (2) tenant purge with a published backup-retention erasure window, (3) a `region` column on the routing map with a **single launch region** (region choice escalated). Per-record erasure is a declared non-goal backed by a no-PII-in-knowledge/audit policy.

---

## Principle 6 restatement draft (H5)

H5 **holds**, provided storage stays embedded (Option A). Under Option B it fails: an external database becomes required infrastructure.

**This is the architect's amended text (review §5, C4), adopted.** It is consistent with S1's external IdP/AS, and it treats Litestream as an optional stateless sidecar with a binary-only fallback.

> **6. One binary, minimal required infrastructure.** The same server binary fills every role (router, cell, local single-project). The data plane needs only that binary, local disk, and an object store for continuous backup. There is no external database, queue or cache service. Identity is delegated to an external authorization server by design, so that signing keys and client secrets stay out of Unimatrix (Principle 8). A backup replicator, if one is used, is a stateless sidecar. A binary-only fallback (snapshot + upload) always exists. Clients remain adapters: the Node edge client, never the binary.

Intent kept: the data plane has no infrastructure dependency the solo operator must run. The change is that "zero" becomes "object storage (+ delegated identity)", and "container optional / daemon+UDS" stops being the defining property.

## H1 challenge

H1 ("one hosted SaaS replaces local deployment entirely") is **separable from the storage decision only under Option A**. With per-project SQLite, a local install can be the same binary running the HTTP daemon with one `[[projects]]` entry. *Correction (C5):* that is the single code path only for the HTTP daemon. The local UDS/stdio path keeps a direct path-hash binding outside the unified resolver by vnc-038 ADR-006 (#5087), and stdio still runs the legacy single-store tick. So "local N=1" in V2 should mean the HTTP daemon with one project, with UDS/stdio either removed or kept dev-only. Resolve this together with H1 / S1 G16. Under that definition, keeping it *working* costs roughly the existing tests. It also provides the exit/portability path (download your project file and run it), a dev/test harness, air-gap capability, and a trivial V1→V2 data migration (file copy plus schema migration; no re-keying). Recommended reframe: **"SaaS-first; local N=1 remains a working, unsupported-as-product byproduct of the same binary."** Removing local code paths would save little. Choosing Option B would force H1 by side effect. Escalated.

---

## Reversibility assessment

| Decision | Door | Cost to reverse | Note |
|---|---|---|---|
| Per-project SQLite data plane (A) | Two-way | **High but bounded.** Port ~800 SQL sites (≈300 outside the store crate) and re-prove isolation, chain and audit. Data migration is mechanical (each DB → rows + project_id) | ass-047's "2–3 engineer-weeks" for a full data-plane port is likely low today given the server-side SQL spread [M] |
| Shared Postgres (B) | **Near one-way** | Very high. Data must be re-split per project, per-tenant restore rebuilt, local N=1 rebuilt | Also forecloses H1 alternatives |
| Control plane SQLite → Postgres (A→C) | Two-way, cheap | ~1 engineer-week (ass-047 Q3) | Keep it behind a trait from day one |
| Cells + router + routing map | Two-way | Low | A routing row per project |
| Residency/scheduler/eviction | Two-way | Low (internal) | Must keep the full-rebuild oracle (ass-104) |
| Litestream for continuous backup | Two-way | Low | It only gives full recovery once vectors are in SQLite (C1). It is gated on the checkpoint PoC (C12). Fallbacks: `VACUUM INTO` snapshot, `export --slug`, volume snapshot; libSQL/Turso if the PoC fails |
| Vectors-in-SQLite (BLOB column) | Two-way | Low | Additive schema; HNSW stays the index |
| **Shared scope implemented as cross-store SQL (ATTACH/joins)** | **One-way trap** | Would couple stores and force B | **S3 must model shared scope as its own store with read fan-out** (ass-060 "owner store"), never as cross-DB SQL |

**The most expensive decision to reverse is B. The most likely way to back into it by accident is S3's shared-scope design.**

---

## Unanswered Questions

1. **Networked-DB latency for the chatty read path**: not measured, because no Postgres instance was available. Only the statement count (~55/search, ~33 synchronous reads [M12]) is empirical. The +10–33 ms figure is [E] and not used for ranking.
2. **Realistic light-tenant tick cost**: only a 192-entry short-content fixture was available. The light-realistic cost model row is [E]. It needs a real second corpus (Med Search) to measure.
3. **Post-fix tick cost** (gated contradiction scan, bounded S1, NLI on new pairs only): it can't be measured until built. The "after fixes" cost row is [E]. It is the number S7 most needs. Measure it at delivery as an acceptance criterion.
4. **Warm-project RSS plateau**: RSS was still rising after 11 minutes and two passes [M7]. The steady-state per-project footprint under long uptime (M9: 1.33 GB for one project after 3 days) and its split between page cache, allocator fragmentation and the ORT arena need a long-run profile.
5. **Litestream compatibility with the store's checkpoint behaviour** (`wal_autocheckpoint=1000`, `PRAGMA wal_checkpoint(TRUNCATE)` in compaction): not verified. It needs a proof-of-concept before Litestream is adopted.
6. **Control-plane HA** and router↔cell trust: out of S2 depth; partly S1.

---

## Out-of-Scope Discoveries

1. **Contradiction scan re-embeds the whole active corpus.** It takes 83–89 s wall at ~15 cores per heavy project, every 4th tick and at tick 0 on every boot [M4]. This is ass-104 I-15 with numbers. It is the single largest cost line in the V2 model, **and its output is status-only**: `ContradictionScanCacheHandle` is read only by `StatusService::compute_report` and feeds `context_status`, not ranking. Gating it is cost-free for retrieval.
2. **S1 graph enrichment** (`graph_enrichment_tick.rs`, an `entry_tags` self-join with GROUP BY/HAVING ≥3) takes ~28 s single core per heavy project every tick and writes **0 edges** in steady state [M4]. This is pure waste at today's cadence.
3. **NLI arena growth.** One graph-inference pass took process RSS from 0.5 to 5.2–9.4 GB, and it was not released [M8]. This is a cell OOM risk independent of tenant count.
4. **Background ML degrades foreground search 4×** (p50 12→50 ms) [M11]. In SaaS this is cross-tenant interference. Check whether query embedding shares the rayon/ORT pool with tick jobs.
5. **When a tick pass exceeds the interval, ticking becomes continuous at full CPU** (observed with 20 light projects at a 5 s interval). The same happens at 900 s with ≈17 heavy projects [D].
6. **Compaction never runs in multi-project mode.** Every shutdown logs `skipping compact: outstanding Store references`, and the production store is 67% free pages (101 of 151 MB) [M13]. This inflates disk and backup size ~3×.
7. **Router map is fixed at boot.** `project register` needs a restart. This blocks S1 onboarding ("signup → project → attach").
8. **sqlx uses one OS thread per SQLite connection** (2 idle, ~6 warm per project [M1/M7]). Threads are bounded only by residency.
9. **Chain-head anchoring (KI-CHAIN-XV-STRONG, #5563) matters more in SaaS.** From the tenant's view the operator *is* the raw-DB-write adversary. Giving tenants their per-project chain heads (in export receipts or periodically) turns that into something they can check themselves. → S3/S7.
10. **42% of the production corpus is quarantined and never purged** (2,505 of 5,933 entries) [M]. ass-104's missing purge lifecycle has a direct storage-cost consequence at SaaS scale.

11. *(architect review)* **Per-slug `AdaptationService` state is never persisted in multi-project HTTP mode.** It is built fresh per slug (`http_provision.rs:237`); only the daemon handle is loaded and saved (`main.rs:975-983`, `shutdown.rs:208-213`). Meanwhile HNSW keeps vectors adapted under the lost state. After a restart, or under eviction, the query embedding space drifts from the indexed vectors. This is a V1 latent defect: check its magnitude before relying on adaptation in V2.
12. *(architect review)* **Per-slug HNSW is dumped only on graceful shutdown** (`infra/shutdown.rs:181-205`). Any crash loses every vector added since the last clean stop until heal re-embeds (V1 and V2). This is the root of correction C1.

---

## Escalations for human

1. **H1, the fate of local deployment.** Options: (a) remove local code paths; (b) **SaaS-first, local N=1 kept working but unsupported as a product (recommended)**; (c) keep local as a supported product. Under Option A, (b) costs about the existing tests and gives the exit/portability and migration path.
2. **Launch region and residency.** Which single launch region (EU or US)? Is data residency a V2 launch requirement, or a later second-region capability?
3. **Erasure posture.** Is a published backup-retention window (e.g. 30 days) acceptable as the erasure SLA, and is "no per-record erasure; no personal data in knowledge/audit" acceptable given the Med Search public-chat surface? This may need legal input.
4. **Scale and HA target for V2 launch.** Is 1,000 tenants a launch target or a horizon? Is a single-cell, no-HA launch acceptable? (Outage on box failure; RTO = restore + fleet re-embed until vectors are in SQLite.) This sets whether the Q2 fixes and Option C are launch-blocking.
   - **E4 addendum (architect): RPO for acknowledged writes.** Async replication (Litestream, 60 s) means a box failure loses up to the RPO of knowledge writes that were already *acknowledged*. The project chain rolls back with them: still valid, but truncated. NX-DURABILITY (#5595) covers shedding under load, not loss on failure, so this needs an explicit acceptance. Is a non-zero RPO (≤60 s) acceptable for V2 launch, or must integrity writes be durable off-box before they are acknowledged? Off-box-before-ack costs write latency or synchronous replication.

---

## Recommendations Summary

- **Q1 (storage)**: Use **per-project SQLite + per-project HNSW as the data plane, refined with entry embeddings persisted in SQLite** (BLOB column), so the SQLite file is the sole durable per-project artifact. Add a SQLite control plane in the same binary behind an async trait, and upgrade it to Postgres only on an HA trigger. Reject shared Postgres + pgvector for V2. The database choice moves cost by tens to hundreds of $/mo; isolation, proven per-tenant restore and zero data-plane migration decide it.
- **Q1 (cost)**: With current tick algorithms, 1,000 tenants ≈ 13 × 16-core cells ≈ €3,050/mo [E]. Gating the status-only contradiction scan alone gives ≈ 7 cells [E]. Adding the S1 bound, NLI on new pairs only, and change-driven gating gives ≈ 1–2 cells ≈ €235–470/mo plus ≤ $193/mo backup [E]. Background ML, not storage, is the cost driver.
- **Q2 (hot path)**: Adopt the architect-amended Principle 7, "in-memory hot path **per resident project**", rebuildable without inference:
  - lazy load with build-on-load (≥0.7 s cold start [D, lower bound]; degrades past the bound);
  - LRU eviction that loses no durable state;
  - a runtime-mutable routing map;
  - a smaller per-project SQLite cache/pool;
  - a **change-driven, fair-share tick scheduler** with fixes in order: (1) gate the status-only contradiction scan, (2) bound S1, (3) NLI on new pairs only.

  One model copy per cell with a bounded NLI arena. Graph rebuilt on a debounced dirty flag with the full rebuild kept as oracle.
- **Q3 (scale)**: Use **tenant-pinned cells behind a thin router**, with no intra-project sharding and no Kubernetes. Launch as one cell with router and control plane in one process, plus Litestream→R2 (gated on the checkpoint PoC). Failover RTO = restore + fleet re-embed until vectors are in SQLite, then restore + rebuild. RPO = sync interval, which loses acknowledged writes (E4 addendum). Cell moves must carry a consistent HNSW dump and adaptation state until vectors are in SQLite.
- **Q4 (export/offboarding/residency)**: Ship tenant-level async export (JSONL + chain report + optional SQLite file) and a tenant purge that covers control plane, backups and logs, with a published backup-retention erasure SLA. Put `region` on the routing map, with a single launch region. Per-record erasure is a declared non-goal under Principles 1/2.
- **Principle 6**: Adopt the architect-amended text: one binary; data plane = binary + local disk + object store; identity delegated to an external AS; replicator is an optional stateless sidecar with a binary-only fallback. H5 holds only under embedded storage.
- **H1**: "SaaS-first; local N=1 kept working but unsupported". Local N=1 means the HTTP daemon with one project; the UDS/stdio path is separate by vnc-038 ADR-006 and should be removed or kept dev-only. Escalated.
- **Reversibility**: B is the near-one-way door. **S3 must model shared scope as a separate store with fan-out, never cross-store SQL**, or the campaign backs into B.

Unimatrix entries consulted: #5591 (C3), #5567 (NX-STORE), #5595 (NX-DURABILITY), #5581 (RETAIN), #5714 (BACKUP-RESTORE), #5592 (N7), #5563 (KI-CHAIN-XV-STRONG), #2153/#2151 (dual-pool), goal entries #5681, #5682, #5684, #5689, #5690, #5732, #5734, #5836. Post-review: architect's citations (#5086, #5087, #5167, #5168, #4129) taken as cited, not re-fetched.
