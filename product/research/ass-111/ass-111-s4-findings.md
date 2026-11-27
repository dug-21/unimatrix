# FINDINGS: ass-111 S4 — Client classes & learning isolation

**Spike**: ass-111 S4 (campaign ass-111, GH #1050)
**Date**: 2026-11-26
**Approach**: evaluation (code + ecosystem)
**Confidence**: directional. Every learning-path claim was checked against working-tree code (`file:line` below). No PoC was run.
**Agent**: researcher-ass-111-s4

Unimatrix entries consulted: #5796 (SL-OPTOUT), #5826 (vnc-050 ADR-004 read tokens record no learning), #5548 (KI-CONTRADICT), #5791 (KI-QUARANTINE), #5528 (SLN1 poisoning), #5792 (C19 headless attach). The briefing and searches also surfaced #5552, #5556, #5560, #5808, #5813, #5814 and #5820; I read their summaries only. Prior findings built on: S1 (final), S5, S6. Other sources: ass-092 and ass-057 findings, GH #1015 and #1013. No Unimatrix writes.

---

## Ground truth: where learning and ranking state is written today (code-verified)

Every later answer depends on this inventory. "Gated" means the vnc-050 predicate `cap_ceiling::records_learning(ceiling)` (`mcp/cap_ceiling.rs:70`) suppresses the write for `read` tokens.

### A. Request-time writes (traffic-driven)

| # | Signal (ranking input it feeds) | Write site | Read-token gate |
|---|---|---|---|
| A1 | `access_count`, `last_accessed_at` (confidence usage and freshness), helpful/unhelpful votes, confidence recompute | `UsageService::record_mcp_usage` → `store.record_usage_with_confidence` (`services/usage.rs:146-293`). Called from search `tools.rs:819`, lookup `:964` (weight 2), store `:1184`, get `:1331`, briefing `:2215` (weight 0, skipped at `usage.rs:370`) | **Gated**: `usage.rs:127` early return; every call site passes `learning_allowed` (`tools.rs:831,976,1196,1343,2227`) |
| A2 | `co_access` pairs (→ CoAccess graph edges → PPR; query-time boost) | `usage.rs:292` (MCP), `usage.rs:338` (hook injection), `uds/listener.rs:1790` (hook ContextSearch) | MCP: gated. Hook path: **not gated in the function**; only the observe route's 403 protects it (`http/router/handlers.rs:50-63`) |
| A3 | `query_log` (→ PhaseFreqTable, eval) | `tools.rs:837-865` (MCP search), `uds/listener.rs:1757-1775` (hook) | MCP: gated. Hook: route-level only |
| A4 | `injection_log` + session injection history (→ effectiveness → utility delta and auto-quarantine) | `uds/listener.rs:1706-1752` | Route-level only |
| A5 | `feature_entries` (work attribution) | `usage.rs:284,351`, only when `write_capable` (store) | Gated, and needs Write |
| A6 | Session signals: `confirmed_entries` (`tools.rs:989,1354`); topic votes (col-017) `record_topic_signal` (`uds/listener.rs:977,1120,1236,1468`); majority-vote feature attribution at eviction and close (`infra/session.rs:983-993`) | `infra/session.rs` | confirmed: gated. Topic votes: hook-only, so route-level |
| A7 | Session-close implicit helpfulness: SIGNAL_QUEUE → `run_confidence_consumer` → `helpful_count` | `uds/listener.rs:2183` (close), `:2457-2492` (enqueue), `:2499-2531` (drain) | Hook-only, so route-level |
| A8 | MicroLoRA adaptation prototypes (the embedding pull applied at store and query time) | `adapt_service.update_prototypes` at `services/store_ops.rs:264`, `services/store_correct.rs:112`. The query is adapted at `services/search.rs:679` | Store/correct need Write. *Training* (`try_train_step`) is reachable only from `server.rs:877 record_usage_for_entries`, which has **test-only callers**, so it is dead in production |

### B. Tick-driven mutations (the per-slug `BackgroundJob` set, `background/jobs.rs:34-362`; legacy `run_single_tick` in `background.rs:428-777`)

| # | Mutation | Site | Depends on |
|---|---|---|---|
| B1 | Effectiveness classification, then **auto-quarantine** after N bad cycles (status change) | `background.rs:1131-1300`, `process_auto_quarantine` `:1406`. Source is `injection_log` (`unimatrix-store/src/read.rs:1212-1280`) | A4 traffic |
| B2 | **Confidence refresh**: rewrites `entries.confidence` for up to 500 entries/tick | `services/status.rs` `run_maintenance` (`:1021`, refresh ≈`:1232-1290`) | A1 counters **and wall clock**: `freshness_score(…, now)` (`unimatrix-engine/src/confidence.rs:184-208,275-299`, half-life 8760 h) |
| B3 | Empirical prior `alpha0/beta0` and adaptive `confidence_weight` (the rerank blend) | `status.rs` ≈`:1294-1350` | Vote population, spread of all confidences |
| B4 | Co-access staleness cleanup | `status.rs` ≈`:1219` | Wall clock |
| B5 | Vector prune (quarantined), heal re-embed, HNSW compaction | `status.rs` `:1036-1218`; `unimatrix-vector/src/index.rs:375` (`compact`) | Status, stale ratio. Rebuild re-randomises HNSW levels (`index.rs:306`) |
| B6 | Co-access → `CoAccess` edges | `run_co_access_promotion_tick`, `background.rs:527` | A2 |
| B7 | TypedGraphState rebuild (PPR, suppression, penalties) | `background.rs:541` / `run_typed_graph_rebuild :968` | All edges. **One-tick lag** after any edge or status write |
| B8 | PhaseFreqTable rebuild | `background.rs:604` / `:1013` | A3 with `phase IS NOT NULL`, **lookback-days window (wall clock)** |
| B9 | Extraction: observations → **new auto-stored entries** | `extraction_tick`, `background.rs:718` | Hook observations |
| B10 | Graph inference: Path A Informs (cosine), Path B NLI Supports, Path C cosine Supports | `run_graph_inference_tick`, `background.rs:754`; `services/nli_detection_tick.rs` | Corpus. Budget-capped per tick, so it converges over several ticks |
| B11 | Graph enrichment: S1 (tags), S2 (vocabulary), **S8 (search co-retrieval read from `audit_log`)** | `run_graph_enrichment_tick`, `background.rs:768`; S8 `services/graph_enrichment_tick.rs:291-330` | S1/S2: corpus. S8: audit rows |
| B12 | Contradiction scan → cache (status/Lambda only, not ranking) | `background.rs:657` | Corpus |

### C. Query-time variability

- **C1** NLI rerank `try_nli_rerank` runs when enabled. `FusionWeights::effective` re-normalises weights when NLI is absent (`search.rs:156-210`). This is inert today because `w_nli = 0.0`.
- **C2** The co-access boost reads `co_access` with a wall-clock staleness cutoff (`search.rs:1158-1204`). Inert while `w_coac = 0.0`, but still executed.
- **C3** `use_fallback` cold-start switches penalties and skips Contradicts suppression (`search.rs:733-790, 1375`).
- **C4** HNSW is approximate. Ties are broken by HNSW order through a stable sort (`search.rs:1349`).
- **C5** The query embedding passes through the ONNX model plus MicroLoRA (`search.rs:679`).
- **C6** The search rate limit is **keyed on the self-asserted `agent_id`**: 300 searches/h and 60 writes/h (`server.rs:599`, `services/gateway.rs:118-131`).

### Isolation by construction (cross-tenant)

All learned state is built per project: store, usage_dedup, `adapt_service` (explicitly "PER-SLUG INDEPENDENT", `http_provision.rs:234-255`), effectiveness, confidence state, typed graph, phase table. Only the stateless embed and NLI handles and the rayon pool are shared. So **cross-tenant learning influence is structurally impossible today**, except through whatever shared-scope semantics S3 adds. That property must be kept: MicroLoRA, priors and graph state may never be pooled across projects (the Hard constraint).

---

## Findings

### Q: "What client classes must V2 serve — interactive harness, autonomous agent (GitHub Actions / task-oriented), chat application over MCP, public web chat — and what does each need (capabilities, hooks, identity)?"

**Answer**: V2 needs **five** classes, not four. "Autonomous agent" splits into a *task agent* (CI or a routine acting on a repo) and an *ingestion pipeline* (bulk writer, e.g. the Med Search sync agent), because their write volume, learning posture and determinism needs differ. The class is a **server-side property of the credential/grant** (S1 R4/R6), never the client-reported `clientInfo.name`.
- For OAuth principals, the class is derived from the registered OAuth `client_id`: Unimatrix's own CLI/bridge versus claude.ai's CIMD URL. This is server-verifiable.
- For tokens and workloads, it is a field on the token record or the trust policy.

**Client-class matrix**

| Class | Examples | Credential / principal (S1) | Agent tier (S1/S6) | Capabilities ceiling | Hooks / observe | Proactive delivery | Workflow context | Learning contribution (default) |
|---|---|---|---|---|---|---|---|---|
| **K1 Interactive harness** | Claude Code, OpenCode, Codex | OAuth `User` + device-key attestation | T2 (CC, OC), T1 (Codex) | role ∩ scope: R, S, W, SessionWrite | **Yes**: observe channel, injection, PreCompact, topic signals | Full: hook injection, briefing, phase-conditioned | Declared cycle/phase; topic inference allowed | **Full**: usage, co-access, votes, session signals, phase tables |
| **K2 Chat application over MCP** | claude.ai connector, Claude Desktop, other MCP chat apps | OAuth `User` (CIMD/DCR client) | T1 | R, S, optional W (Writer role) | **No** (no adapter). Observe denied: the class grant lacks SessionWrite | Briefing on request only. No hook injection | None, unless the user explicitly calls `context_cycle` | **Usage only**: access and explicit votes. No session, phase or topic signals |
| **K3 Task agent** | GitHub Actions job, cloud routine, scheduled task | `Workload` (GH OIDC → RFC 8693) or `Service` token | T1 (T2 if a harness with an adapter runs inside CI) | Policy mode: R/S or R/S/W on the bound project. Never Admin | Only if a harness runs in the job (then K1 rules apply to that session) | Briefing/search on request | May declare a cycle (S5 W4) | **Explicit only**: stores and corrections are content, not learning. Usage and co-access **off** by default, because repetitive queries are non-representative and inflate counts |
| **K4 Ingestion pipeline** | Med Search sync agent, importers | `Service` or `Workload` | T1 | R, S, W + deprecate/correct/edge; **needs a non-Admin retraction path** (Q4) | No | None | Optional cycle per ingest run (`fetch → extract → reconcile → publish`) | **Off**. Typically paired with a deterministic project (Q3) |
| **K5 Public-chat backend** | Med Search web backend relaying anonymous users | `Service` token, mode `read` | T1 | R, S only | **No**. Observe 403 | None | None. No session, no cycle | **None, and not Owner-configurable** |

**Per-class needs that V1 does not meet**
- **Quotas per principal, not per `agent_id`.** The rate limiter keys on a self-asserted string (`server.rs:599`). K5 (one principal, many end users) hits 300 searches/h. K4 hits 60 writes/h. Any caller can bypass either limit by rotating `agent_id`. V2 needs per-principal, per-class quotas, Owner-set within platform caps.
- **The class decides learning contribution independently of the capability mode** (Q2). Today the only switch is `read` vs `read-write`.
- **K2 and K5 need briefing and search without session plumbing.** This works today: `index_briefing.rs` runs without phase (S5), and MCP calls with no session produce no session signals.
- **K3 and K4 need large writes without the hook channel.** This works today over MCP.

**Knowledge-engine boundary**: none of the classes gives Unimatrix scheduling, triggering or coordination. CI and pipelines *call* Unimatrix; Unimatrix never invokes them. Webhooks and notifications to clients are out (S5 R4).

**Recommendation**: Adopt K1–K5 as the V2 client-class model. Store `client_class` and `learning_contribution` on the grant (token record, trust policy, or OAuth client registration), set by the Owner within per-class bounds (K5 is fixed at none). Replace agent_id-keyed rate limiting with per-principal, per-class quotas.

---

### Q: "How is each class's traffic kept out of (or selectively into) learning and access-pattern signals? Is vnc-050 `read` mode sufficient for public chat, and how is anonymous/public access reconciled with 'every port authenticated'?"

**Answer**:

**Is `read` mode sufficient for public chat?** For *learning isolation* on today's code, **yes, with one latent hole that must be closed by rule**. As a *general* isolation mechanism, **no**: it ties "may write" to "learns", so it cannot express K2, K3 or K4.

**What `read` mode demonstrably blocks** (verified):
- **MCP tool path.** All A1/A3/A6 sinks are gated for the five retrieval and write tools (sites in Ground truth A). A `read`-token search, lookup, get or briefing writes no usage, vote, co-access, query_log or confirmed-entry rows. A `helpful` vote from a read token is dropped (`usage.rs:127`).
- **Hook/observe path.** It is gated only at the route (`handlers.rs:50-63`, a 403 before dispatch). That is sufficient while the route is the only entry, but the sinks in `uds/listener.rs` (A2–A4, A6, A7) carry **no predicate of their own**.
- **No query text is persisted.** `query_log` is gated. The `search_service` audit row stores only a count (`search.rs:1437-1447`). `http_access` stores request metadata. End-user questions therefore do not persist in Unimatrix: a PII win for K5.

**What `read` mode does not block**
1. **Audit-log-derived learning (latent hole).** S8 graph enrichment reads `audit_log` rows to mint `CoAccess` edges (`graph_enrichment_tick.rs:311`). Audit rows are written for read-token calls by design (ADR-004 #5826: "audit rows still written"). Any learning source that reads the audit log **bypasses the sink predicate by construction**.
   - Today S8 is dead in production: it filters `operation = 'context_search'`, but the search path writes `operation = "search_service"` (`search.rs:1442`), and the MCP search handler emits no tool audit row.
   - The obvious one-line "fix" would open a public-chat → graph leak.
2. **Time-driven drift** (B2 freshness, B4, B8 window). Public chat does not cause it, but read mode does not stop results changing. Isolation is not determinism; that is Q3.
3. **The rate limit, the only per-caller state public traffic touches.** It is keyed on a forgeable `agent_id` (C6). This is an abuse issue, not a learning one.

**Selective learning: the rules**

| Rule | Statement | Enforcement point |
|---|---|---|
| **LI-1 One predicate, three inputs** | `learning_allowed = ceiling_permits(Write) ∧ grant.learning_contribution ≠ none ∧ project.learning_mode = adaptive`. It extends `cap_ceiling::records_learning` and is computed once in `build_context`, so there is still one policy source (#4453). | `mcp/cap_ceiling.rs:70` + ToolContext |
| **LI-2 Sinks enforce, not routes** | Every learning sink takes `learning_allowed` (and the contribution level), including the hook-dispatch sinks in `uds/listener.rs` (A2–A4, A6, A7). The observe 403 stays as defence in depth, not as the only gate. | `UsageService`, listener sinks |
| **LI-3 The audit log is never a learning source** | No tick may derive ranking state from `audit_log` / `http_access`. S8 must be rebuilt on `query_log` (already gated) or deleted. Principle 2 keeps audit complete, so audit must stay learning-inert. | `graph_enrichment_tick.rs:291` |
| **LI-4 Contribution levels** | `full` (K1): A1–A7. `usage` (K2): A1 (access and explicit votes) + A2 + A3 without phase. `explicit` (K3): no implicit signals; explicit `helpful` votes only if the Owner enables them. `none` (K4, K5): nothing. | LI-1 predicate, carried as a level |
| **LI-5 Workflow-position signals only from declared-workflow sessions** (closes S5 W3) | Phase tables (already `phase IS NOT NULL`), the category histogram, the goal-cluster boost, `feature_entries` and topic votes accrue only from sessions with declared or harness-observed workflow context (K1, or K3 with a declared cycle). Usage, co-access and votes are **not** workflow-scoped: they stay useful for non-workflow clients and are governed by LI-4 instead. | `query_log` filters; session registry |
| **LI-6 Topic-vote inference gated by class** (closes S5 W1 / col-017) | `record_topic_signal` and majority-vote attribution (`session.rs:983-993`) run only for K1 sessions on the observe channel, never for Service or Workload principals, and never when the session declared a cycle (declared beats inferred, as today). Today the signals originate only in hook dispatch (`listener.rs:977,1120,1236,1468`), so K2/K5 cannot trigger it now. The gate makes that a rule rather than an accident of transport. | listener dispatch |
| **LI-7 Signals are attributed to the principal** | Every learning row carries `principal_id` (S1 G9). An Owner can then purge one principal's contributions, and per-principal bounds (SLN1 #5528) become enforceable. Session keys are namespaced `(tenant, project, principal, session)` (S1 R6/G12). | store schema (additive) |
| **LI-8 Learning never crosses the project boundary** | Signals land only in the store of the project addressed in the URL. Learned models (MicroLoRA, priors, graph) are never pooled across projects. Shared-scope reads are S3's question. The S4 constraint is that a reader's traffic must not write ranking state into a store that another tenant ranks from without an explicit grant. | per-slug construction (`http_provision.rs:234-255`) |

**Anonymous and public access versus "every port authenticated"**
- There is **no anonymous Unimatrix port**. The public web user authenticates (or not) to Med Search's web app. Med Search's backend calls Unimatrix with a K5 `read` service token (S1 E4).
- Abuse controls for anonymous users (captcha, per-IP limits) belong to Med Search. Unimatrix enforces a per-principal quota for the K5 principal.
- Unimatrix does not accept an end-user identifier. An opaque per-user reference would add a quasi-identifier to the audit log without any authorization value.

**Recommendation**:
- Keep vnc-050 `read` as the K5 mechanism.
- Generalise it to LI-1 through LI-8: one three-input predicate, enforced at every sink including the hook dispatcher.
- Ban audit-log-derived learning (LI-3) before anyone "fixes" S8.
- Gate col-017 topic inference by client class (LI-6).
- Attribute every learning row to a principal (LI-7).

---

### Q: "What does a deterministic (learning-off) project mode require end to end — votes, co-access, confidence evolution, graph self-enrichment, NLI, tick — so the same query yields the same results?"

**Answer**: Learning-off alone is not enough.

**Why read-only traffic is not deterministic.** Even if every caller used a `read` token, results would still drift:
- The B2 freshness decay uses wall-clock `now`.
- B3 re-derives the confidence blend weight.
- B10/B11 keep minting cosine and tag edges for several ticks after each write.
- B7 lags edge writes by one tick.
- B5 compaction re-randomises HNSW.

**Definition (the testable contract).** Results are deterministic when the same query, with the same parameters and no session context, returns byte-identical, identically ordered results and scores **for equal fingerprints**. The fingerprint is `(corpus_epoch, derived_epoch, config_version, model_digest, index_build_id)`:
- `corpus_epoch` advances on every entry, status or edge write (store, correct, deprecate, quarantine, edge, re-embed).
- `derived_epoch` is the corpus epoch that the in-memory derived state (typed graph, index) reflects.
- Results are **settled** when `derived_epoch == corpus_epoch`.

This is Med Search's "same results unless the library changed" (#1015), stated in a form that can be verified. The #5796 `done_when` ("same query before/after 100 other queries") is a special case of it.

**End-to-end requirements** (each one maps to the inventory above)

| # | Requirement | Covers | Change |
|---|---|---|---|
| D1 | All request-time learning sinks are off for the project, whatever the principal | A1–A7 | LI-1 third input (`project.learning_mode = deterministic`). Hook sinks need LI-2 |
| D2 | Votes accepted but inert | A1 votes, A7 | Recorded in audit only. Not applied to `helpful_count` |
| D3 | Confidence is a pure function of corpus state | B2, B3 | Deterministic weight profile: `w_usage = w_fresh = w_help = 0` (base, correction and trust re-normalised), recomputed only on entry write. Skip the B2 refresh and B3 prior/`confidence_weight` updates; pin `confidence_weight` to a constant |
| D4 | Only explicitly written edges exist | B6, B10, B11, A2 | Disable the co-access promotion, graph inference (A/B/C) and enrichment (S1/S2/S8) jobs for the project. The per-slug `BackgroundJob` seam (`background/jobs.rs`) is the natural per-project switch. Graph ⊆ `source='agent'` edges (#5796 `done_when` 2; complements #1013 provenance) |
| D5 | No engine-initiated status changes or entries | B1, B9 | Disable auto-quarantine and extraction. Effectiveness `w_util = 0` |
| D6 | Embedding pinned | C5, A8 | Pin the model digest. MicroLoRA identity (no adapter) and no prototype pull for the project. Re-embed only on an explicit Owner action, which bumps the corpus epoch |
| D7 | Exact, totally ordered retrieval | C4, B5 | Exact (brute-force) kNN for deterministic projects, or a frozen HNSW build that is compacted only as an epoch event. Total order `(final_score desc, entry_id asc)` instead of HNSW-order ties. At 1e5 entries × 384 dims, exact search is ≈4e7 MACs per query (directional, not measured) |
| D8 | No query-time optional ML | C1, C2 | NLI rerank off. Co-access boost not computed (`w_coac = 0` and skip the prefetch). Workflow terms (phase, histogram, goal) forced to 0 (S5 W2 parity) |
| D9 | Derived state tracks writes, not ticks | B7, C3 | Rebuild the typed graph on write (debounced) for deterministic projects, and serve `settled=false` with the fingerprint until `derived_epoch` catches up. Without this, results change one tick after every edge or status write |
| D10 | Fingerprint returned on every search, lookup and briefing response | contract | Lets the hypothesis agent record and check reproducibility (Med Search's audit need) |
| D11 | Hygiene keeps running | B4, B5 prune/heal, retention, session GC | Learning-off does **not** mean tick-off. Prune and heal are corpus events that bump the epoch |

**Graceful degradation (Principle 5) under D-mode.** Optional ML is *off*, not degraded, so there is no silent fallback that changes results. The only required model is embedding, and it already fails explicitly when absent. The fallback is defined and visible, so Principle 5 holds.

**Residual risk (unvalidated).**
- ONNX numeric reproducibility across heterogeneous fleet hosts and thread counts. Same-process determinism is tested (`unimatrix-embed/src/onnx.rs:500`); cross-host is not. With S2's fleet, a query re-embedded on a different CPU could flip near-ties.
- Mitigations: pin deterministic projects to one instance class, set `intra_op_num_threads = 1`, or quantise similarities (for example to 1e-6) before ordering. This needs measurement before the contract is claimed.

**Cost to the self-learning goal.** Deterministic mode turns off exactly the features the self-learning goal sells, so it must be a per-project mode with adaptive as the default (#1015 option a). Most of its machinery is shared with LI-1/LI-2 (the project input to the predicate) and with the per-slug job switch, so the marginal build is D3, D7, D9 and D10.

**Recommendation**: Define deterministic mode by the fingerprint contract above. Implement it as `project.learning_mode = deterministic` (a PCD key, S5 Q4) that drives D1–D11. Prove it with the #5796 acceptance test extended to "equal fingerprints ⇒ identical results across 100 interleaved queries **and** N ticks **and** wall-clock advance". The human decides whether to build it (E1).

---

### Q: "What does Med Search need for frequent contradiction/retraction (KI-CONTRADICT, quarantine, NLI) and is it a V2 requirement or a domain extension?"

**Answer**: The **mechanics** are V2 core. They are engine integrity features (KI-CONTRADICT #5548 and KI-QUARANTINE #5791 already sit under the integrity goal), they are domain-agnostic, and several have gaps that block Med Search today.
- **Vocabulary and policy** (research categories and edges, how contradictions are served, NLI on/off) are **domain-pack/PCD configuration** (S5 Q4).
- **Automatic NLI contradiction detection** is an **optional domain extension**, not a launch requirement.
- **Retraction feeds, appraisal, hypothesis generation and publishing** are **Med Search's application**, outside Unimatrix (knowledge-engine boundary).

**Med Search requirements mapped to V1 capabilities**

| # | Need | V1 state (verified) | Gap | Classification |
|---|---|---|---|---|
| M1 | Record contradictions between claims | `context_edge` Contradicts, written **bidirectionally** with `source='agent'` (`mcp/edge_write.rs:200-230`). Write capability | none | Core (exists) |
| M2 | Serve contradictions correctly | `suppress_contradicts` hides the lower-ranked end (`unimatrix-engine/src/graph_suppression.rs:44`; `search.rs:1381`). Skipped during cold-start `use_fallback` | For research, competing claims must be **shown together and annotated**, not suppressed; suppression by rank is wrong when rank is the disputed thing. Needs a per-project policy `contradiction_serving: suppress \| annotate` (annotate returns both, each with `contradicted_by`) | Core mechanism + PCD policy key |
| M3 | **Retraction**: a retracted paper's entries stop being served by default and stay retrievable by ID with status | Quarantine excludes from all read surfaces (#1009 fixed; `tools.rs` lookup filter) but **requires Admin** (`tools.rs:2257`), and V2 grants Admin only to human Owners (S1). Deprecate needs only Write (`tools.rs:1688`), but flexible search **still serves deprecated entries with a penalty** (`search.rs:757-790`) | **No non-Admin path to exclusion.** The K4 sync agent cannot retract. Needs either (a) a per-project PCD policy `deprecated_visibility: penalize \| exclude` plus a structured deprecation reason (`retracted`, with a citation), or (b) a Write-tier withdraw disposition that has quarantine's visibility semantics. Escalated (E3) | Core |
| M4 | Know what depends on a retracted entry | Supports/Prerequisite edges exist. ass-057 proposed `stale_dependency_edges` in status (not built) | A "dependents of deprecated/withdrawn" query (context_graph filter, PL-11 #5794) | Core (PL-11) |
| M5 | Updated findings replace old ones with provenance | `context_correct` supersession chain, hash-chained (Principles 1–2) | none | Core (exists) |
| M6 | Machine-inferred links never look appraised | `graph_edges.source` already records origin (`agent`, `nli`, `cosine_supports`, `co_access`, `S1`, `S2`, `S8`; `unimatrix-store/src/read.rs:1751-1803`) | Traversal and serving cannot exclude by origin (#1013) | Core (#1013) |
| M7 | Automatic contradiction detection | NLI substrate live but off. The Contradicts write was removed and the score is discarded (ass-092: restoring it is a small change in Path B) | Restore it (ass-092 Option B). Under deterministic mode its output must not enter traversal (D4). The useful form for Med Search is **candidate contradictions for agent review**: inferred-origin edges excluded from serving, which the sync agent confirms by writing a `source='agent'` edge | **Domain extension** (optional ML, off by default, Principle 5 fallback = today) |
| M8 | High-frequency ingest | Write rate limit 60/h, keyed on `agent_id` (`gateway.rs:118-131`) | Per-principal quotas (Q1) | Core |
| M9 | Reproducible results for the hypothesis agent | none (#1015 pending) | Q3 deterministic mode | Core mode (decision pending, E1) |
| M10 | Public chat over Med Search's library that never influences learning | `read` tokens (vnc-050) | LI-1…LI-8 | Core |
| M11 | Research vocabulary (claim, evidence, hypothesis, …; contradicts/supports/cites) | ass-057 mapping; DA10 #1011 open | Project-declared categories and edges via the PCD | Domain pack (`research` / `med-search` template, S5 C7) |
| M12 | PubMed/Retraction Watch monitoring, claim extraction, hypothesis generation, publishing | n/a | none in Unimatrix | **Med Search application.** Not engine |

**V2 requirement or domain extension?**
- **V2 requirements** (also needed by SDLC, so not Med-Search-specific): M2 policy, M3, M6, M8, M10, plus the M9 mode if E1 is approved.
- **M3 is the only item that blocks Med Search outright.** Today a non-human principal cannot take an entry out of default retrieval, and V2's "Admin is human-only" rule makes that permanent unless a Write-tier path exists.
- **M7 (NLI detection)** is a post-launch domain extension.

**Recommendation**:
- Treat M2, M3, M6 and M8 as V2 core integrity work and sequence M3 first.
- Put the contradiction-serving and deprecated-visibility policies in the PCD.
- Ship NLI contradiction detection (ass-092 Option B) after launch, as a provenance-tagged *candidate* source, excluded from serving in deterministic projects.
- Keep retraction monitoring and hypothesis work in Med Search.

---

## Escalations for human

- **E1 — SL-OPTOUT / deterministic mode (#1015, #5796; decision pending).** I recommend option (a): build it as a per-project mode with adaptive as the default, defined by the fingerprint contract (Q3).
  - Why: V2 is explicitly multi-domain, and LI-1/LI-2 build most of the plumbing anyway.
  - The vision would then state learning as a per-project choice.
  - This is a product-identity call against the self-learning goal, so the human decides.
- **E2 — Default learning contribution for K2 (chat connector) and K3 (CI task agent).** I recommend K2 = `usage`, K3 = `explicit`, K4/K5 = `none`. Whether a human's claude.ai conversations should train their project's ranking is a product choice.
- **E3 — Retraction authority (M3).** Should a Write-capable *service* principal be able to remove an entry from default retrieval?
  - Option (a): PCD `deprecated_visibility: exclude` plus a structured `retracted` reason (no new status). Recommended.
  - Option (b): a new Write-tier "withdrawn" disposition.
  - Option (c): keep exclusion Admin-only, which blocks Med Search.
  - This touches S1's "Admin only to human Owners" rule.
- **E4 — Public chat has no Unimatrix identity, and no end-user reference is passed through** (Q2). This confirms S1 E4 and extends it.

---

## Unanswered Questions

- **Cross-host ONNX reproducibility** (Q3 residual risk). Needs measurement on the S2 fleet shape. Reason: requires an experiment across instance types or thread counts, beyond a directional spike.
- **Exact-kNN cost for deterministic projects at Med Search scale.** The ≈4e7 MACs/query at 1e5 × 384 figure is arithmetic, not measured. Reason: S2 owns hot-path cost. Med Search corpus size was not available.
- **Shared-scope learning** (whose store a shared-entry access signal lands in). Reason: S3 scope. S4 contributes only LI-8.
- **Whether K2 OAuth `client_id` reliably distinguishes harness from connector** when the harness bridge and claude.ai use different registration methods (CIMD vs DCR). Reason: depends on the IdP choice (S1 E1) and needs a live test.
- **Per-principal quota values per class.** Reason: pricing and capacity are S2/S7.

---

## Out-of-Scope Discoveries

- **S8 graph enrichment is dead in production.** It selects `audit_log.operation = 'context_search'` (`graph_enrichment_tick.rs:312`), but no production path writes that operation: search writes `"search_service"` (`search.rs:1442`) and the MCP handler emits no tool audit row. Only tests and import fixtures write it. Fixing the name would leak read-token traffic into CoAccess edges (LI-3). Delete S8 or re-source it from `query_log`.
- **`context_cycle_review` has no `require_cap`.** Any authenticated caller, including a `read` token, can trigger `store_cycle_review` (`tools.rs:3553`) and the transcript-purge pipeline (`tools.rs:2444-3560`). This violates Principle 3. Worth a V1 bugfix.
- **MicroLoRA training is unreachable in production.** `try_train_step` and `record_training_pairs` are called only from `server.rs:877 record_usage_for_entries`, whose callers are all in tests. Query and store embeddings are adapted by static or loaded weights plus store-time prototype pulls. Either remove the dead path or wire it in deliberately. #1015's "no adaptive query embedding" request is already almost true.
- **Hook-path learning sinks have no predicate** (`uds/listener.rs:1706-1790, 2457-2531`). They rely only on the observe-route 403. Any future second entry into hook dispatch (OpenCode v2 plugin route, a stateless 2026-07-28 transport) would bypass vnc-050. Captured as LI-2.
- **Rate limiting keys on the self-asserted `agent_id`** (`server.rs:599`). It is forgeable and gives a per-principal bound no meaning. This bears on SLN1 (#5528).
- **Default confidence weights include a wall-clock freshness term** (`w_fresh = 0.18`, half-life 8760 h). Every project's ranking therefore drifts with time even with zero traffic. This is not a bug, but it should be stated wherever "results change only from usage" is claimed.

---

## Recommendations Summary

- **Q1 (client classes)**: Serve five classes: K1 interactive harness, K2 chat app over MCP, K3 task agent, K4 ingestion pipeline, K5 public-chat backend. Store the class and learning contribution on the server-side grant (OAuth client registration, token record or trust policy), never on `clientInfo`. Replace agent_id-keyed rate limits with per-principal, per-class quotas.
- **Q2 (learning isolation)**: Keep vnc-050 `read` for K5 (verified sufficient on the MCP path). Generalise it to one predicate, `ceiling ∧ grant.learning_contribution ∧ project.learning_mode`, enforced at **every** sink including the hook dispatcher (LI-1/LI-2). Ban audit-log-derived learning (LI-3, S8). Use contribution levels full / usage / explicit / none (LI-4). Workflow-position signals come only from declared-workflow sessions (LI-5), and topic-vote inference is limited to K1 (LI-6). Attribute learning rows to the principal (LI-7) and never write learning across projects (LI-8). There is no anonymous port: public users reach Unimatrix only through the app backend's read token.
- **Q3 (deterministic mode)**: Define it as "equal fingerprint `(corpus_epoch, derived_epoch, config_version, model_digest, index_build_id)` ⇒ byte-identical ordered results". Implement it as a PCD `learning_mode = deterministic` that disables all request-time sinks and votes; freezes confidence to non-usage, non-time terms; keeps only agent-written edges by turning off promotion, inference and enrichment jobs; turns off auto-quarantine and extraction; pins the model with identity MicroLoRA; uses exact kNN with id tie-break and no optional query-time ML; rebuilds derived state on write with a `settled` flag; and keeps hygiene ticks running. Build decision: E1.
- **Q4 (Med Search)**: Contradiction and retraction mechanics are V2 core. The blocking gap is **M3: there is no non-Admin way to exclude a retracted entry**; sequence it first (E3). Also: contradiction-serving policy suppress/annotate (M2), origin-excludable edges (#1013, M6), per-principal ingest quotas (M8). NLI contradiction detection is a post-launch domain extension that emits provenance-tagged candidates (ass-092 Option B). Vocabulary goes in a research/med-search PCD template. Retraction monitoring and hypothesis generation stay in Med Search.
