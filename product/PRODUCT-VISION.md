# Unimatrix — Product Vision & Roadmap

---

## Vision

Unimatrix is a workflow-aware, self-learning knowledge engine built for agentic workflows such as software delivery. It makes knowledge curation a first-class activity in the workflow itself — not a side effect. Agents search, store, and correct knowledge entries as a normal part of doing work: decisions get attributed, lessons get captured, patterns get refined. Unimatrix makes that knowledge trustworthy, consistent, and — as it learns from actual usage — continuously more relevant.

Two surfaces, both driven by the same engine: agents retrieve knowledge on demand (search, lookup, get), and Unimatrix delivers it proactively — phase-conditioned injections and briefings that surface what matters before agents need to ask. The combination of explicit curation and self-improving delivery is what makes it distinct.

Unimatrix is not an orchestration engine. It does not coordinate agents, schedule
work, or manage workflows. It is a knowledge engine that understands workflow context
— your current phase, what your team has been doing, what comes next — and uses that
understanding to surface relevant knowledge at exactly the right moment.

The key mental model: workflow definitions, agent definitions, and skill definitions
are static — they live in your tooling and change infrequently. Architecture
decisions, patterns, and lessons-learned are dynamic — they evolve with every
feature, every delivery, every failure. Unimatrix was designed to manage the dynamic
layer. Every architectural pivot, every hard-won lesson, every reusable pattern is
captured, attributed, and made available to every future agent that needs it.

Built for agentic software delivery. Configurable for any workflow-centric domain.

---

## Story

Unimatrix began in agentic software delivery, where the problem was specific: AI agents forget, contradict each other, and confidently repeat mistakes. We built a knowledge engine where nothing is merely stored — everything is attributed, hash-chained for integrity, scored by real usage, and correctable with full provenance. Agents stopped relitigating decisions. Knowledge started improving with every delivery.

That foundation became a platform. A typed knowledge graph formalizes relationships — not just what agents retrieve together, but why: support, contradiction, supersession, dependency. A confidence system learns from actual usage rather than manual calibration. Contradiction detection is semantic. Any event source — hooks, webhooks, automated pipelines — feeds the learning layer without agent cooperation. Any knowledge-intensive domain — environmental monitoring, SRE operations, scientific research, regulatory compliance — runs on the same engine, configured not rebuilt.

The intelligence pipeline is the core of the platform. It is not a retrieval engine with additive boosts. It is a session-conditioned, self-improving relevance function: given what the agent knows, what they have been doing, and where they are in their workflow, surface the right knowledge — before they ask for it. The graph, the confidence system, the observation pipeline, and the GNN are all inputs to this function. The function learns. Every session makes it better.

Secured with OAuth, containerized, serving any number of repositories from a single instance. The integrity chain runs through all of it: hash-chained corrections, immutable audit log, trust-attributed provenance — tamper-evident from first write to last.

---

## The Critical Gaps

Before the roadmap, a clear-eyed list of where Unimatrix has strayed from its domain-agnostic foundations, and where new surface area has been accumulated. Status reflects current state.

### Domain Coupling
| Gap | Severity | Status |
|-----|----------|--------|
| Time-based freshness in Lambda — domain-specific assumption | Critical | **Resolved** — freshness dimension dropped from Lambda entirely (#520); Lambda is now a 3-dimension structural health metric (graph, contradiction, embedding) |
| "lesson-learned" category name hardcoded in scoring | Critical | **Fixed** — W0-3 `boosted_categories` config |
| Lambda dimension weights hardcoded | Critical | **Fixed** — W0-3; W3-1 will learn them |
| SERVER_INSTRUCTIONS const uses dev-workflow language | High | **Fixed** — W0-3 `[server] instructions` config |
| Initial category allowlist hardcoded | High | **Fixed** — W0-3 / dsn-001 |
| `context_cycle_review` tool name is SDLC-specific | Medium | **Fixed** — renamed to `context_cycle_review` |
| HookType enum tied to Claude Code events | Medium | **Fixed** — col-023 / W1-5 (PR #332) |
| trust_source vocabulary dev-flavored | Low | Open |
| Observation metrics schema (bash_for_search, etc.) | Low | **In progress** — W1-5 `domain_metrics_json` |

### Security
| Gap | Severity | Status |
|-----|----------|--------|
| Auto-enroll gives read access to any unknown process | High | Configurable via `PERMISSIVE_AUTO_ENROLL`; W2-3 OAuth closes it fully |
| agent_id per-call model: friction, unreliable, spoofable | High | W2-3 OAuth path |
| No token-based client identity for STDIO | High | W2-3 JWT path |
| No path to OAuth for centralized deployment | Medium | W2-3 |

### Scalability & Architecture
| Gap | Severity | Status |
|-----|----------|--------|
| Process exits on session end | Critical | **Fixed** — W0-0 daemon mode |
| Single SQLite writer | High | **Fixed** — W0-1 dual-pool sqlx |
| No backup/recovery story | High | W2-1 container packaging |
| No container deployment model | Medium | W2-1 |
| No HTTP transport — stdio only | Medium | W2-2 |
| Graph rebuilt at query time, not persisted | Medium | **Fixed** — W1-1 GRAPH_EDGES persistence |

### Intelligence & Confidence
| Gap | Severity | Status |
|-----|----------|--------|
| Confidence weights hardcoded — cannot adapt | High | Interim fix W0-3; W3-1 learns them |
| Only supersession edge type — no typed relationships | High | **Fixed** — W1-1 RelationEdge |
| Contradiction detection uses cosine heuristic, not NLI | Medium | **Fixed** — W1-4 NLI cross-encoder |
| Graph edges not persisted — lost on restart | Medium | **Fixed** — W1-1 GRAPH_EDGES |
| Co-access and contradiction never formalized as graph edges | Medium | **Fixed** — W1-1 |
| Intelligence pipeline is additive boosts, not a learned function | High | **Roadmapped** — Wave 1A + W3-1 |
| No session-conditioned relevance — every query treated identically | High | **Roadmapped** — Wave 1A + W3-1 |
| No proactive delivery — all surfaces are reactive | High | **Roadmapped** — Wave 1A (WA-4) |
| GNN training loop has no closed feedback signal | High | **Deferred** — WA-3 deferred; W1-5 behavioral signals cover initial W3-1 training |

---

## Wave 0 — Prerequisites — COMPLETE

All Wave 0 items are complete. Brief summaries for context.

### W0-0: Daemon Mode — COMPLETE (`vnc-005`, PR #295)
Unimatrix runs as a persistent background daemon on a Unix Domain Socket (0600 permissions). Survives MCP client disconnection. Auto-start on first connection. PidGuard + flock enforce one-daemon-per-project. Enables all background tick work — write queue draining, ML inference, GNN training — to run continuously between sessions.

### W0-1: sqlx Migration — COMPLETE (`nxs-011`, PR #299)
Replaced `rusqlite` + `Mutex<Connection>` with `sqlx` + dual connection pool (`read_pool` 6-8 connections, `write_pool` 2). Analytics writes routed through a bounded async write queue (batched ≤50 events or 500ms). All `spawn_blocking` DB call sites removed — storage is async-native. Positions the codebase for PostgreSQL with no application logic rewrite.

### W0-2: Session Identity — DEFERRED
Deferred; design analysis showed no security value before OAuth. W2-3 JWT `sub` claim is the correct non-spoofable identity anchor. See ADR #2267.

### W0-3: Config Externalization — COMPLETE (`dsn-001`)
Categories, freshness half-life, confidence weights, server instructions, cycle parameter labels, and agent bootstrap config all externalized to `config.toml`. Two-level hierarchy (global + per-project, replace semantics). The single unlock for domain agnosticism — operators configure a new domain without code changes.

---

## Wave 1 — Intelligence Foundation
*Estimated: 4-5 weeks, after Wave 0. Runs before and in parallel with Wave 2.*

### W1-1: Typed Relationship Graph — COMPLETE (`crt-021`, PR #316)
Upgraded `StableGraph<u64, ()>` to `StableGraph<u64, RelationEdge>`. Edges persisted to `GRAPH_EDGES` in `analytics.db`. Bootstrapped from existing co-access and shadow evaluation data. `RelationType` stored as string (not integer discriminant) for extensibility. Bootstrap edges carry `bootstrap_only=1` flag — excluded from confidence scoring until NLI confirms or refutes.

In-memory cache follows `Arc<RwLock<_>>` tick-rebuild pattern. Search hot path reads from memory only.

---

### W1-2: Rayon Thread Pool + Embedding Migration — COMPLETE
**What**: Dedicated `rayon::ThreadPool` for all CPU-bound ML inference, bridged to tokio via `oneshot` channel. ONNX embedding migrated off `spawn_blocking` as the first consumer.

All ML inference (W1-4 NLI, W3-1 GNN) runs on the dedicated rayon pool. Panics in rayon closures drop the oneshot sender, returning `Err` to the awaiting async task — no cross-thread panic propagation. `spawn_blocking` handles only I/O-bound operations thereafter.

GGUF (W2-4) uses a separate bounded rayon pool to prevent long synthesis runs from starving NLI inference.

---

### W1-3: Evaluation Harness
**Business outcome**: Every intelligence change is measured against real query scenarios before reaching agents. Regressions caught before production. The human sees exactly what changed and why.

**Six deliverables:**

**1. `unimatrix snapshot`** — Full DB copy via `VACUUM INTO`. `--anonymize` replaces agent/session IDs with seeded consistent pseudonyms for safe fixture commits.

**2. `unimatrix eval scenarios`** — Mines `query_log` from a snapshot into eval scenario JSONL. Soft ground truth from result_entry_ids at query time. Supports hand-authored scenarios with hard-labeled expected IDs.

**3. `unimatrix eval run`** — In-process A/B comparison. Opens snapshot read-only, constructs one `ServiceLayer` per profile config, replays scenarios through each. Computes P@K, MRR, Kendall tau, rank deltas, latency per scenario.

**4. `unimatrix eval report`** — Markdown comparison report: aggregate summary table, notable ranking changes, latency distribution, zero-regression checklist.

**5. `UnimatrixUdsClient` (Python)** — Connects to running daemon's UDS socket. Identical tool API to the stdio client. Enables eval against live production daemon.

**6. `UnimatrixHookClient` (Python)** — Sends synthetic lifecycle and observation events to the hook IPC socket. Validates the observation pipeline and GNN training signal quality without requiring Claude Code.

**Gate condition for W1-4**: Eval results show measurable improvement on a representative query set before model ships.
**Gate condition for W3-1**: Hook simulation client validates GNN training label quality on synthetic behavioral patterns before production deployment.

**Security requirements:**
- [High] Eval mode operates on DB snapshot copy only — `eval run` refuses the live daemon DB path. Open snapshot with `?mode=ro`.
- [High] `unimatrix snapshot --anonymize` required before any snapshot is committed.
- [Medium] Scenario input files validated: max 10,000 scenarios, max 2,000 chars per query.

**Effort**: ~1.5–2 weeks (offline eval ~1 week; live simulation layer ~3–4 days; mixed Rust + Python).

---

### W1-4: NLI + Cross-Encoder Re-ranking — COMPLETE (`crt-023`, PR #328)
**What**: One small ONNX cross-encoder (~85MB, NLI fine-tuned) in two modes: (1) post-store contradiction/support detection between new entries and HNSW neighbors; (2) search re-ranking of top HNSW candidates against the actual query.

**Pipeline with W1-4:**
```
query → embed → HNSW top-20 → NLI re-rank → co-access boost → return top-K
```

Post-store NLI runs fire-and-forget off the MCP hot path. Contradiction > threshold writes `Contradicts` edge to GRAPH_EDGES (`source='nli'`, `bootstrap_only=0`). Entailment writes `Supports` edge. NLI confidence score stored in `metadata` for W3-1 GNN edge features.

Bootstrap edge promotion from W1-1 handled as a first-tick background task.

Circuit breaker on NLI → auto-quarantine: cap `Contradicts` edges per tick. NLI-derived auto-quarantine requires higher confidence threshold than manual correction.

Graceful degradation: absent or hash-invalid model file → server starts on cosine fallback with logged warning.

---

### W1-5: Observation Pipeline Generalization — COMPLETE (`col-023`, PR #332, GH #331)
**Business outcome**: Any domain can connect its native event stream to the learning layer without code changes.

**What**: Replace `HookType` closed enum with `ObservationEvent { event_type: String, source_domain: String, payload: JsonValue, session_id: String }`. Generalize `UniversalMetrics` so dev-specific metrics become the "claude-code" domain pack's metrics, not hardcoded struct fields. Rewrite all 21 detection rules to operate on the generic event schema. Implement config-file-driven domain pack registration loaded at startup.

**Key constraints:**
- No wire protocol changes — `ImplantEvent` is already generic
- Backward compatibility — "claude-code" default pack produces identical retrospective output
- Extraction rule sandboxing — pure data transformations, no env/fs access
- Payload validation — max 64KB, nesting ≤ 10 levels at ingest
- `source_domain` validated `^[a-z0-9_-]{1,64}$`

**Why this matters for Wave 1A**: W1-5 is the behavioral signal collection layer. Without it, the observation pipeline generates only Claude Code-specific training labels. With it, W3-1 trains on domain-neutral behavioral signals from any event source.

**Effort**: 5-7 days (detection rule rewrite is the long tail).

---

## Wave 1A — Adaptive Intelligence Pipeline
*Runs after W1 foundation; completes before Wave 2 deployment begins*

The intelligence pipeline cannot learn from usage it cannot observe, cannot predict what agents need without knowing where they are in the cycle, and cannot close the feedback loop without knowing when retrieval fails. This wave builds the complete signal collection, feedback closure, and proactive delivery infrastructure that makes W3-1 a genuinely session-conditioned, self-improving relevance engine — not just a weight calibrator.

The three knowledge delivery surfaces — UDS injection, `context_briefing`, and `context_search` — are all outputs of this intelligence pipeline. They share the same session context vector, the same candidate scoring, and ultimately the same learned function. Wave 1A builds that shared foundation using computable formulas; W3-1 replaces those formulas with a learned function that keeps improving.

**WA-0 comes first.** Before adding session-conditioned signals to the ranking pipeline, the pipeline's existing signals must be fused correctly. Adding more additive terms to a structurally broken formula makes the problem worse.

---

### WA-0: Ranking Signal Fusion — COMPLETE (`crt-024`, PR #336)
Six-term fused linear combination replaces the sequential NLI sort + co-access re-sort pipeline. All signals normalized to [0, 1] and weighted proportionally via `[inference]` config. NLI is dominant at `w_nli=0.35` as W3-1's initialization point. `apply_nli_sort` removed; `compute_fused_score` pure function is W3-1's feature vector interface.

**Shipped formula** (sum=0.95, 0.05 headroom for WA-2):
```
score = w_nli(0.35)*nli + w_sim(0.25)*sim + w_conf(0.15)*conf
      + w_coac(0.10)*coac_norm + w_util(0.05)*util_norm + w_prov(0.05)*prov_norm
final = score * status_penalty
```
Eval: 1,761 scenarios, P@5 0.3060, MRR 0.4222, zero true regressions. Follow-up: #337 (merge_configs re-validation gap).

---

### WA-1: Phase Signal + FEATURE_ENTRIES Tagging — COMPLETE (`crt-025`, PR #338)
**Business outcome**: The engine knows where each agent session is in its workflow — not because agents declare it, but because the workflow coordinator signals it as part of normal orchestration. Every piece of knowledge stored is annotated with the phase that produced it.

**What**: Add `type: "phase"` event to `context_cycle`, alongside existing `start` and `stop`.

```
context_cycle(type: "start",  topic: "crt-024")
context_cycle(type: "phase",  topic: "crt-024", phase: "design")
context_cycle(type: "phase",  topic: "crt-024", phase: "implementation")
context_cycle(type: "stop",   topic: "crt-024")
```

The `phase` string is opaque to Unimatrix — stored as metadata, not interpreted, no vocabulary enforced. Consistency within a workflow is the only requirement.

**SessionState change**: `current_phase: Option<String>`, updated on each phase event, cleared on stop.

**FEATURE_ENTRIES schema**: Add `phase TEXT` (nullable) column. When an entry is stored, the active `current_phase` is written alongside it. Entries stored before any phase transition get `phase: null`.

**Protocol integration**: SM calls `context_cycle(type: "phase", ...)` at existing protocol checkpoints. Design session: `scope → design → synthesis`. Delivery session: `pseudocode → implementation → testing → delivery`. Retro: `retro`. No new agent discipline required beyond the SM.

**Why this matters**: `current_phase` is the explicit phase dimension of the session context vector. `FEATURE_ENTRIES.phase` is the supervised training data for W3-1 — without it, the GNN cannot learn category→phase→usefulness correlations from real workflow history.

**Implementation scope**: `context_cycle` validation, `SessionState.current_phase`, FEATURE_ENTRIES schema migration (nullable column, non-breaking), `context_store` path writes active phase, protocol updates at existing checkpoints.

**Effort**: ~1 day.

---

### WA-2: Session Context Enrichment (ASS-028 Recommendation 1)
**Business outcome**: The ranking pipeline sees the current session's activity — what phase it is in, what kind of knowledge it has been producing — and uses that signal to surface more relevant results immediately, without any ML.

**What**: Add `category_counts: HashMap<String, u32>` to `SessionState`. Call `record_category_store(session_id, category)` after each successful `context_store`. Thread `session_id` into `ServiceSearchParams`. Apply a phase-conditioned category affinity boost in `SearchService` as a final ranking step after NLI re-ranking and co-access boosting.

**The complete ranking pipeline after this feature:**
```
HNSW(k=20) → NLI re-rank → co-access boost → category affinity boost → top-k
```

**Affinity boost formula (cold-start, manual — replaced by W3-1):**

When `current_phase` is set (explicit signal from WA-1):
```
boost = phase_category_weight(entry.category, current_phase) * 0.015
```

When only histogram is available (implicit fallback):
```
p(category) = count(category) / total_stores
boost = p(entry.category) * 0.005
```

The explicit phase signal is 3× the implicit histogram signal. Both are additive, bounded. The histogram is a fallback and a smoother — it reflects what the session has actually been doing, not just the last declared phase. When both signals are present, both apply. This weighting becomes W3-1's cold-start initialization, not permanent architecture.

`AFFINITY_WEIGHT` constants are config-driven from the start — hardcoding them would repeat the pattern this whole wave is designed to eliminate.

**Cold start**: empty histogram → zero boost for all entries. Existing behavior preserved exactly.

**UDS injection**: At phase transitions and PreCompact, include a summary of the session's category histogram in the synthesis injection: `"Recent session activity: decision × 3, pattern × 2 (design phase signal)"`. Informational — agents can use it or ignore it.

**Effort**: ~1 day.

---

### WA-3: MissedRetrieval Signal — DEFERRED
**Status**: Deferred. Revisit after W3-1 has trained on W1-5 behavioral signals and explicit helpfulness votes, and label coverage is assessed via the W1-3 eval harness.

**Why deferred**: The similarity-threshold predicate (`sim > 0.75`) is insufficient — a high-similarity entry may have been correctly excluded by confidence, co-access affinity, NLI score, or phase-category mismatch. Using the full fused score as the predicate instead is circular: it generates training labels derived from the formula W3-1 is replacing. W1-5 behavioral signals (re-search, rework, successful phase completion) already provide entry-specific negative labels grounded in actual agent behavior, without counterfactual inference. Additionally, W3-1 operates on the HNSW candidate set — entries that never enter top-20 are an embedding problem, not a ranking problem, and W3-1 cannot fix them regardless. Revisit only if W1-3 eval demonstrates a demonstrated coverage gap in training labels that behavioral signals cannot fill.

---

### WA-4: Proactive Delivery — COMPLETE (`crt-027`, PR #349)
**Business outcome**: Agents receive relevant knowledge before they search for it. UDS injection is phase-conditioned, not generic. `context_briefing` becomes the targeted handoff at each phase transition. The system is no longer purely reactive.

**What**: Two related features that implement proactive and comprehensive delivery modes using the session context built in WA-1 and WA-2.

#### WA-4a: Phase-Conditioned Proactive Injection

Replace the generic semantic search used for UDS injection candidates with a session-conditioned candidate set, rebuilt at each phase transition and drawn from on subsequent hook events.

**Phase-transition cache** (rebuilt when `current_phase` changes):
```
candidate_set = Active entries where:
  topic = current feature_cycle
  AND category ∈ expected_categories(current_phase)
  AND entry_id NOT IN session.injection_history

ranked by:
  confidence
  + co_access_affinity(entry, injection_history_entries)
  + phase_category_boost(entry.category, current_phase)
```

On each PreToolUse hook: draw top-1 (or top-2 if budget allows) from the cached candidate set. No full-graph scan at hook time. Cache invalidated and rebuilt on phase transition. This is the proactive query mode — no user query, session context IS the retrieval anchor.

**Why phase-transition cache not per-hook scoring**: Scoring all Active entries on every hook event is expensive and unnecessary. Phase changes infrequently; the candidate set is stable within a phase. The cache trades minor staleness for a clean hot path.

#### WA-4b: Phase-Conditioned `context_briefing`

Resurrect `context_briefing` as a targeted surface triggered by the SM at phase transitions, not called ad-hoc by agents.

**New retrieval path in `context_briefing`**:
- Filter by `topic` (current feature cycle)
- Phase-condition the ranking using `current_phase` and category affinity
- Return structured top-k results — decisions, patterns, procedures relevant to this phase
- No injection_history filter — this is the comprehensive "here's what you need entering this phase" set

**SM orchestration pattern**:
```
SM: context_cycle(type: "phase", phase: "implementation", topic: "crt-024")
    context_briefing(topic: "crt-024")  ← immediately after
```

`context_briefing` returns the top-k entries for (topic=crt-024, phase=implementation), giving agents a structured knowledge handoff at each phase boundary without requiring them to search.

**Effort**: ~2 days total (WA-4a: candidate cache management, hook integration; WA-4b: new briefing retrieval path, SM protocol updates).

---

### WA-5: PreCompact Transcript Restoration (ASS-028 Recommendation 2) — COMPLETE (`crt-028`, PR #357)
**Business outcome**: Context window compaction no longer erases the recent conversation. The last few user prompts and assistant responses are restored alongside the Unimatrix briefing synthesis, giving the model continuity through the compaction boundary.

**What**: The `PreCompact` hook already receives `transcript_path` in stdin JSON. This field is parsed but never used. Read the transcript file locally (no server round-trip) before sending the `CompactPayload` request:

1. Open `input.transcript_path`
2. Scan backward from end of file
3. Collect last k `{user_text, assistant_text}` pairs — `type: "user"` records with `type: "text"` content blocks (skip tool_result blocks)
4. Format as a "Recent conversation" block
5. Prepend to the server's `BriefingContent` response before writing stdout

**Extraction pattern (reverse scan)**:
```
for line in reverse(transcript_lines):
    if type == "assistant": collect text blocks
    if type == "user": collect text blocks (type: "text" only)
        when both collected: push pair, k--
    if k == 0: stop
```

**Output format**:
```
=== Recent conversation (last 3 exchanges) ===
[User] {prompt text}
[Assistant] {response text}
...
=== End recent conversation ===
```

**Budget**: Separate injection limit for PreCompact (recommended: 3000 bytes) rather than the general 1400-byte cap. PreCompact is the highest-value injection point in the system — more context at compaction is directly valuable.

**Scope**: All logic in `uds/hook.rs`. No server changes. Fully independent — ships in any order relative to WA-1 through WA-4.

**Effort**: ~1 day.

---

### ASS-029: GNN Architecture Spike — Required Before W3-1
**Status**: Not yet started. Research prerequisite — W3-1 delivery cannot be scoped without this.

**What**: A focused research spike that defines the GNN's architecture for three distinct query modes, the session context feature vector, the training batch structure, and the tick scheduling model. Without this, W3-1 cannot be properly estimated or implemented.

Full research scope: `product/research/ass-029/SCOPE.md`.

**Why this is a gate**: The expanded W3-1 scope (session-conditioned relevance function across three query modes) is architecturally different from the original W3-1 spec (weight vector learning). The forward pass differs by mode. The training batch structure must account for MissedRetrieval events and phase-labeled FEATURE_ENTRIES. The tick scheduling must compose with existing compaction, NLI, and confidence refresh work. These decisions compound — getting them wrong at design time cascades through all of W3-1.

**Outputs from ASS-029**:
1. GNN forward pass architecture per query mode
2. Session context feature vector specification (complete, typed)
3. Training batch construction from all three signal sources
4. Candidate set management strategy for proactive mode
5. Tick scheduling and resource envelope
6. Cold-start initialization from WA-2 manual formula

---

## Wave 2 — Deployment
*Planning in progress. See `product/WAVE2-ROADMAP.md` for the full Wave 2 planning document, updated goal statements, and research spike prerequisite list (ASS-041 through ASS-047).*

*Original design notes below remain as detailed specification reference. Goal statements and scope are being revised by the Wave 2 research spikes.*

Wave 2 delivers containerization, HTTPS transport, multi-project routing, OAuth, and the enterprise admin console. These are independent of the intelligence pipeline — they are the delivery infrastructure that makes Unimatrix accessible beyond a single developer workspace. Wave 1A is complete. The Wave 2 enterprise tier ships as BSL-1.1; the OSS STDIO tier remains MIT/Apache.

### W2-1: Container Packaging
**Business outcome**: Knowledge survives infrastructure changes — production-grade deployment with clean backup, recovery, and standard container lifecycle.

**What**: Dockerfile + docker-compose with separate named volumes:
- `unimatrix-knowledge` — `knowledge.db` (back up frequently; integrity-critical)
- `unimatrix-analytics` — `analytics.db` (back up less frequently; self-healing)
- `unimatrix-shared` — models (re-downloadable), `config.toml` (mount as read-only bind)

Container is stateless except the volumes. Backup = volume snapshot of `unimatrix-knowledge`. `HEALTHCHECK` verifies daemon liveness and schema version currency.

**Security requirements:**
- [High] Named volumes owner-only at container build time (`chmod 0700`)
- [Medium] `config.toml` as read-only bind mount from secrets manager, not in data volume
- [Low] Container runs as non-root (`USER unimatrix`)

**Effort**: 2 days.

---

### W2-2: HTTP Transport + Basic Auth
**Business outcome**: External systems and pipelines call Unimatrix without being Claude Code plugins.

**What**: HTTP/HTTPS transport alongside existing UDS/stdio. `--transport http --port 8080`. Validates `Authorization: Bearer <token>` against AGENT_REGISTRY. Capability check is identical — same service layer, different transport resolution.

**Privileged access separation (ASS-027)**: Two HTTPS listeners on separate ports — content port (8443) and admin port (8444). Admin tools (`context_enroll`, `context_quarantine`, admin-flagged `context_deprecate`) registered on admin port only. Content port exposed via load balancer; admin port internal-only (Kubernetes ClusterIP, VPN-accessible). Network-layer enforcement without SSH or local access.

**Prerequisite**: Verify rmcp 0.16 HTTP transport readiness before committing to estimate.

**Security requirements:**
- [Critical] TLS non-negotiable — no `--insecure` flag; refuse HTTP mode without cert/key paths
- [Critical] Bearer token validation must be constant-time comparison
- [High] Admin port never default — require explicit opt-in; omitting it gives content-only deployment
- [High] Maximum request body (≤1MB), connection timeout (30s), max concurrent connections enforced

**Effort**: 3-4 days.

---

### W2-3: Multi-Project Routing + OAuth Middleware
**Business outcome**: Teams accumulate organizational knowledge spanning projects, with OAuth-enforced access control that scales from personal to enterprise.

**What**: Two-tier store routing (owner + project) and OAuth 2.0 client credentials flow.

Owner store holds cross-project conventions and patterns. Project store holds project-specific knowledge. At query time, search fans out to both (project weighted higher). Write always goes to project store. Promotion to owner tier is explicit, attributed, hash-chained.

OAuth scopes map to capabilities: `unimatrix:search → Search`, `unimatrix:read → Read`, `unimatrix:write → Write`, `unimatrix:admin → Admin`. JWT `sub` → `agent_id` for attribution. Custom `unimatrix_project` claim → project store routing.

`TenantRouter` resolves the correct `Arc<Store>` pair at request time. Tool logic unchanged.

**Security requirements:**
- [Critical] JWT: algorithm allowlist (RS256/ES256 only), enforce exp/iss/aud
- [Critical] `sub` claim validated `^[a-zA-Z0-9_-]{1,64}$`
- [High] `unimatrix_project` claim validated against registered project allowlist — never construct file paths directly from claim values
- [High] OAuth client secrets never stored in any database — only `client_id`

**Effort**: 4-5 days.

---

### W2-4: Embedded GGUF Module
**Business outcome**: The system reasons about what agents need — without any cloud or external LLM dependency. Every deployment, including air-gapped environments, gains local inference.

**What**: Optional `unimatrix-infer` capability — local GGUF model via llama.cpp when configured. Enabled by a single config entry; absent means zero behavior change. All GGUF inference runs on a dedicated rayon pool (separate from the ONNX pool) — GGUF inference is seconds, not milliseconds, and must not starve NLI or GNN training.

When present, qualitatively upgrades:
- `context_cycle_review` — 21 detection rules produce genuinely reasoned recommendations, not pattern-matched findings
- `context_status` — heuristic thresholds become specific, actionable explanations
- Contradiction explanation — NLI gives a score; GGUF gives the *why*
- Background intelligence without any external LLM — synthesis and retrospective analysis run overnight, ready when the next session begins

Build behind Cargo feature flag (`features = ["infer"]`). Validate llama.cpp FFI integration in a proof-of-concept before committing to estimate — platform-specific compilation (ARM, x86), signal handler conflicts, and memory management in long-running processes are known risks.

**Gate condition**: W1-3 eval harness results; proof-of-concept in target environment.

**Security requirements:**
- [Critical] GGUF model file SHA-256 hash-pinned in config — replaced model file is an undetectable reasoning-manipulation vector
- [High] LLM input length-limited (~4000 tokens max); content scanner applied before passing to model
- [High] LLM output passes full content scanner before storage or return

**Effort**: 1-2 weeks (proof-of-concept required before committing).

---

## Wave 3 — Self-Improving Intelligence
*After Wave 1 + 1A complete + ASS-029 + sufficient usage data from W0-0 daemon*

### W3-1: GNN — Session-Conditioned Relevance Function
**Business outcome**: Confidence weights, candidate ranking, and proactive delivery all adapt automatically to each deployment's actual usage patterns. The manual formulas from Wave 1A become the cold start. The GNN refines them continuously from real agent behavior — no manual tuning, no reconfiguration, no redeployment.

**Expanded scope** (see ASS-029 for full design): W3-1 implements a session-conditioned relevance function, not just a weight vector learner. The function replaces the manual ranking formulas across all three delivery surfaces:

```
GNN(entry_features, session_context, interaction_features) → relevance_score

Mode 1 — Proactive (UDS injection):
  session_context as retrieval anchor (no query embedding)
  candidate set = Active entries not in injection_history

Mode 2 — Comprehensive (context_briefing at phase transition):
  session_context at phase transition moment
  candidate set = all entries for this topic

Mode 3 — Reactive (context_search re-ranking):
  session_context fused with query_embedding
  candidate set = HNSW top-20
```

**Session context vector** (fully defined by Wave 1A):
```
[current_phase,             ← WA-1 explicit signal
 category_histogram[k],     ← WA-2 implicit signal
 injection_count,           ← what has been served
 query_count,               ← how many searches made
 topic_embedding,           ← from feature_cycle
 cycle_position_normalized, ← how far into this phase
 rework_event_count]        ← quality pressure signal
```

**Entry features (node)**:
```
[confidence_6_factors, category_embedding, access_count,
 helpful_ratio, correction_count, graph_degree,
 days_since_last_access, nli_edge_confidence]
```

**Interaction features (dynamic)**:
```
[already_served,               ← binary, from injection_history
 co_access_with_served_count,  ← affinity to already-served set
 query_overlap_score]          ← appeared in prior query results this session
```

**Training signal — three sources**:
1. *Explicit*: `helpful_count` / `unhelpful_count` per entry
2. *Implicit behavioral* (W1-5): retrieval → re-search = negative; retrieval → rework event = negative; retrieval → successful phase completion = positive
3. *MissedRetrieval* (WA-3): entry existed, was similar, was never served, agent had to fill the gap themselves — strong targeted negative label

**Cold-start**: Initializes from the WA-2 manual formulas (explicit phase weight 0.015, histogram weight 0.005). As training data accumulates, the GNN refines from there. Config-defined `[confidence] weights` (W0-3) remain the fallback when the GNN has insufficient data.

**In-memory weight cache**: Learned scores cached in `Arc<RwLock<_>>`, rebuilt after each training run on the maintenance tick. Search hot path reads from memory only. Missing or stale GNN output degrades gracefully to the manual formulas.

**Gate condition**: ASS-029 design spike complete; 50+ helpfulness votes OR 2-4 weeks of active daemon use (W0-0) generating behavioral observation data; W1-3 hook simulation client validates training label quality on synthetic patterns.

**Training loop architecture** (defined in ASS-029): when training runs, batch construction, resource envelope, tick scheduling relative to compaction and NLI inference.

**Effort**: 1-2 weeks (no GNN infrastructure exists; ASS-029 must validate effort estimate).

**Security requirements:**
- [High] Per-agent vote-rate limit (max 10 helpfulness votes per agent per hour) before GNN trains on them — prevents synthetic label injection
- [High] Implicit training labels attributed to sessions, not agent_ids
- [Medium] Learned weight vector stored with checksum and training run input hash to detect tampering

---

### W3-2: Knowledge Synthesis — Future Option
**Status**: Deferred. Revisit after W3-1 is deployed and knowledge base entry density is assessed.

**What**: Maintenance-tick process that distills knowledge clusters into single synthesized entries when 3+ Active entries share a topic+category with mutual Supports/CoAccess edges and combined content exceeds a token threshold.

**Hard prerequisites**: W3-1 (for GNN-weighted confidence on synthesized entries) and W2-4 (GGUF module — without an LLM, synthesis is concatenation with an authority label, not actual distillation). W3-2 must not ship without both.

**Gate condition**: Deploy when knowledge base exceeds ~200 clustered entries on any topic AND W3-1 AND W2-4 are deployed. Premature synthesis at low entry counts produces noise.

---

## Dependency Graph

```
Wave 0 (COMPLETE)
  W0-0: Daemon ──────────────────────────────────────────────────────────────┐
  W0-1: sqlx dual pool ──────────────────────────────────────────────────────┤
  W0-3: Config externalization ──────────────────────────────────────────────┤
                                                                              │
              ┌───────────────────────────────────────────────────────────────┘
              ▼
Wave 1 — Intelligence Foundation
  W1-1: Typed graph (COMPLETE) ──────────────────────────────────────────────┐
  W1-2: Rayon pool (COMPLETE) ────────────────────────────────────────────┐  │
  W1-3: Eval harness ─────────────────────────────────────────────────┐   │  │
  W1-4: NLI re-ranking (COMPLETE) ◄──────────────────────(W1-2+W1-3)─┤   │  │
  W1-5: Obs generalization (COMPLETE — col-023, PR #332, GH #331) ──┐│   │  │
                                                                      ││   │  │
              ┌───────────────────────────────────────────────────────┘│   │  │
              │     ┌─────────────────────────────────────────────────┘│   │  │
              ▼     ▼                                                   │   │  │
Wave 1A — Adaptive Intelligence Pipeline                                │   │  │
  WA-0: Ranking signal fusion ◄──────────────────────── (W1-4 done)─┐  │   │  │
  WA-1: Phase signal (#330) ──────────────────────────── (WA-0)─────┤  │   │  │
  WA-2: Session context enrichment (ASS-028 R1) ◄─────────(WA-1)───┤  │   │  │
  WA-3: MissedRetrieval signal ◄───────────────────────── (WA-0)────┤  │   │  │
  WA-4: Proactive delivery ◄─────────────────────────── (WA-1+WA-2)─┤  │   │  │
  WA-5: PreCompact restoration (ASS-028 R2) ────────────────────────┘  │   │  │
  ASS-029: GNN architecture spike ──────────────────────────────────┐  │   │  │
                                                                     │  │   │  │
        ┌────────────────────────────────────────────────────────────┘  │   │  │
        │     Wave 2 (parallel, after Wave 0) ◄─────────────────────────┘   │  │
        │       W2-1: Container                                              │  │
        │       W2-2: HTTP transport                                         │  │
        │       W2-3: Multi-project + OAuth                                  │  │
        │       W2-4: GGUF ◄──────────────────────────────── (W1-2+W1-3)   │  │
        │                                                                    │  │
        ▼                                                                    │  │
Wave 3 — Self-Improving Intelligence                                         │  │
  W3-1: GNN session-conditioned relevance ◄───────── (ASS-029 + 1A + data) ◄┘  │
                          needs W1-5 signals ◄──────────────────────────────────┘
  W3-2: Knowledge synthesis ◄────────────────────── (W3-1 + W2-4 + density)
        [future option]
```

Key sequencing rules:
- WA-0 is the first Wave 1A item — adding session-context signals to a broken fusion formula compounds the problem
- Wave 1A requires W1-5 behavioral signal infrastructure
- W3-1 requires ASS-029 design spike + Wave 1A signal infrastructure + usage data from W0-0
- Wave 2 is independent of Wave 1A — both run after Wave 0; Wave 1A should reach WA-3 before production deployment begins accumulating data without MissedRetrieval closure
- W3-2 requires both W3-1 (learned weights) and W2-4 (GGUF inference for synthesis)

---

## Effort Summary

| Wave | Item | Effort | Gate |
|------|------|--------|------|
| W0 | All items | **COMPLETE** | — |
| W1-1 | Typed graph | **COMPLETE** | — |
| W1-2 | Rayon pool | **COMPLETE** | — |
| W1-3 | Eval harness | ~1.5-2 weeks | W0 complete |
| W1-4 | NLI re-ranking | **COMPLETE** | — |
| W1-5 | Obs generalization | **COMPLETE** — col-023, PR #332, GH #331 | — |
| WA-0 | Ranking signal fusion | ~1-2 days | W1-4 complete; GH #329 subsumed |
| WA-1 | Phase signal (#330) | ~1 day | WA-0 complete |
| WA-2 | Session context enrichment | ~1 day | WA-1 complete |
| WA-3 | MissedRetrieval signal | ~1 day | WA-0 complete |
| WA-4 | Proactive delivery | ~2 days | WA-1 + WA-2 complete; cache strategy decided |
| WA-5 | PreCompact restoration | ~1 day | Independent |
| ASS-029 | GNN architecture spike | ~2-3 days | W1A scoped |
| W2-1 | Container | ~2 days | W0 complete |
| W2-2 | HTTP transport | ~3-4 days | W0 complete; rmcp HTTP verified |
| W2-3 | Multi-project + OAuth | ~4-5 days | W2-2 complete |
| W2-4 | GGUF module | ~1-2 weeks | W1-2 + W1-3; proof-of-concept first |
| W3-1 | GNN relevance function | ~1-2 weeks | ASS-029 + Wave 1A + usage data |
| W3-2 | Knowledge synthesis | ~1 week | W3-1 + W2-4 + entry density |

**Total to adaptive, domain-agnostic, securely deployed platform**: ~10-12 weeks of focused work after Wave 0, including the intelligence pipeline maturation of Wave 1A.

**Wave 3 trails by** however long it takes for daemon usage data to accumulate under W0-0 (typically 2-4 weeks of active deployment). ASS-029 can begin during Wave 1A delivery.

---

## What's Preserved Throughout

Every wave maintains these non-negotiables:

- **Hash chain integrity**: `content_hash` / `previous_hash` on every entry — untouched by any wave
- **Correction chain model**: `supersedes`/`superseded_by` — extended by W1-1 but not modified
- **Immutable audit log**: every operation attributed and logged
- **ACID storage**: SQLite transactional guarantees — W0-1 migrates the driver, not the guarantees
- **Single binary**: all waves add capability to the same binary, not new services
- **Zero infrastructure**: container is optional; daemon + UDS works without it
- **In-memory hot path**: all analytics-derived search data (graph, weights, co-access, GNN scores) cached in `Arc<RwLock<_>>` rebuilt by tick — never read from the database directly at query time
- **Graceful degradation**: every ML capability (NLI, GNN, GGUF) has a defined fallback to the previous behavior when absent, loading, or failed

The integrity chain is the product's defensible moat. The roadmap is designed around it, not in spite of it.

---

## What This Unlocks

**After Wave 0 (COMPLETE)**:
- Background tick, write queue, ML inference, and GNN training run continuously
- Sessions no longer lose accumulated state at process exit

**After Wave 1 + 1A**:
- The intelligence pipeline knows where each session is in its workflow
- UDS injection is phase-conditioned — agents receive relevant knowledge without searching
- `context_briefing` is a targeted phase-transition handoff, not a generic summary
- `context_search` re-ranking is session-conditioned — identical queries return different rankings based on session context
- The GNN training loop has complete signal coverage: explicit votes, behavioral outcomes, and missed retrievals
- Any domain can connect its event stream to the learning layer without code changes

**After Wave 2**:
- Any domain deploys with a config file (SRE, environmental, research, legal)
- External systems integrate via HTTP without being Claude Code plugins
- Multi-project routing — project knowledge + organization-tier conventions served together
- OAuth-gated access for team deployments
- Container deployment with clean backup/recovery

**After Wave 3**:
- Confidence weights, candidate ranking, and proactive delivery all adapt automatically per deployment
- A new domain gets config-defined starting weights from day one and GNN-learned weights within weeks
- The manual ranking formulas from Wave 1A are the cold start — the GNN has long since replaced them
- Reference: a Raspberry Pi 5 running neural-data-platform, fully air-gapped, becomes a self-contained intelligent sensor platform with local reasoning (W2-4 + W3-1)

---

## Security Cross-Cutting Concerns

### Threat Model Evolution

**Wave 0 — daemon-local (hardened)**

Primary risks: local process spoofing via agent_id; UDS socket access by local users; config file injection; auto-enroll granting write to unknown processes. Blast radius: one machine, one knowledge base. Recovery = restore from backup.

**Wave 1 — daemon-local with ML inference**

New threat actors: adversarial knowledge inputs designed to manipulate NLI scoring or GNN training. The knowledge base itself becomes an attack surface.

Risks:
- Vote manipulation to corrupt GNN training labels
- Adversarial entry pairs crafted to maximize false `Supports` edges
- Model file replacement between ONNX integrity checks
- NLI → auto-quarantine feedback loop without circuit breaker

Blast radius: corrupted confidence weights affect every query result. Recovery = retrain GNN from clean observation snapshot + restore knowledge.db from backup.

**Wave 1A — session-conditioned intelligence**

New threat actors: adversarial session context injection. With session context driving ranking, manipulating `current_phase` or `category_counts` could bias retrieval.

Risks:
- Agent injecting false `context_cycle(phase: ...)` events to bias phase-conditioned ranking
- Category histogram manipulation via bulk `context_store` calls with crafted categories
- MissedRetrieval signal poisoning: flooding the analytics drain with synthetic missed events to skew training labels

Mitigations: phase events are SM-sourced (SM has `Privileged` trust by convention); category allowlist constrains histogram; MissedRetrieval events require a real `context_store` call (write capability gate) and HNSW proximity check (cannot be fabricated without a real embedding).

**Wave 2 — HTTP-exposed**

Standard network threat actors apply. TLS, constant-time token comparison, admin port internal-only, request size and connection limits. Blast radius: network-wide exposure of all knowledge content.

**Wave 3 — adaptive intelligence with learned weights**

GNN training data integrity is the primary concern. A corrupted training set propagates into all ranking decisions silently.

Mitigations: per-agent vote-rate limiting (max 10 votes/agent/hour); implicit training labels attributed to sessions not agents; learned weight vector checksummed and input-hashed per training run.

---

### Non-Negotiables Across All Waves

**1. Hash chain integrity is immutable.**
`content_hash` and `previous_hash` on every entry — never skipped, backdated, or made optional. Includes synthesized entries (W3-2), auto-extracted entries, and any entry produced by background maintenance.

**2. Audit log is append-only and complete.**
Every operation that changes `knowledge.db` state produces an AUDIT_LOG entry with `agent_id`, `session_id`, `operation`, `target_ids`, and `outcome`. The analytics write queue must not become an audit bypass.

**3. Capability checks are enforced at the service layer, not the transport layer.**
Whether the caller arrives via UDS, stdio, HTTP bearer token, or OAuth JWT, capability checks happen in the service layer after identity resolution. Transport authentication is a precondition, not a substitute.

**4. Content scanning is not bypassed for machine-generated content.**
`AuditSource::Internal` bypass applies only to content generated by the Unimatrix process itself (confidence updates, usage increments, observation recording). Never applies to synthesized entries (W3-2), NLI edge labels, or config-derived content stored as knowledge.

**5. No secret material in any database.**
OAuth client secrets, API keys, TLS private keys never stored in `knowledge.db` or `analytics.db`.

**6. The UDS session exemption from rate limiting remains local-only.**
`CallerId::UdsSession` rate limit exemption never extends to HTTP transport callers.

**7. Analytics-derived data is never read directly on the search hot path.**
All analytics-derived search data (graph edges, GNN scores, confidence weights, co-access affinities) cached in memory, rebuilt by tick. Direct `analytics.db` reads at query time are an architectural violation.

---

### Architectural Decisions Required Before Wave 2

**Decision 1: Token format for HTTP bearer tokens**
Option A (recommended for W2-2): Opaque tokens in AGENT_REGISTRY (lookup-based). Option B (W2-3 OAuth): Signed JWTs validated locally. Validation must be constant-time at comparison.

**Decision 2: TLS termination**
Option A: Unimatrix terminates TLS directly. Option B: TLS terminated by reverse proxy, Unimatrix binds `127.0.0.1` only. Support both — enforce `127.0.0.1` binding in Option B startup checks.

**Decision 3: Multi-project isolation**
Per-project `knowledge.db` + `analytics.db` for all tiers. Shared `analytics.db` across projects is a cross-project observation leakage risk. Per-project for both is the only safe model.

---

### Design Decisions Required Before Wave 1A Delivery

**Decision 1: Affinity boost fusion rule (WA-2)**
Explicit phase signal weight vs. implicit histogram weight. Proposed: `phase_boost * 0.015 + histogram_boost * 0.005` (explicit 3× implicit). These values become W3-1's cold-start initialization — document the rationale so it can be updated as the GNN learns.

**Decision 2: Proactive candidate cache strategy (WA-4a)**
Full-graph scoring on every hook event vs. phase-transition cache rebuilt when `current_phase` changes. Recommendation: phase-transition cache. Rebuild at phase transition, draw from cache at hook time. Cache invalidated on phase change and explicitly on `context_correct` / `context_deprecate` for cached entries.

---

## Future Opportunities

These are not roadmap items — they are additive and could be picked up after the roadmap waves complete.

### Proactive Knowledge Discovery

`context_cycle_review` and `context_status` already analyze session evidence and have store access at call time. Extending them to surface `KnowledgeCandidate` records — topics that *should* have entries but don't — closes a detection gap. Signal sources: recurring queries with low top-similarity across 3+ sessions; re-derivation sequences (searched → low results → succeeded → no store issued); co-access clusters with no synthesizing entry. With W2-4 (GGUF): candidates include a draft entry from the local LLM. Candidates are never auto-stored — they require explicit `context_store` from a human or privileged agent.

This capability is partially addressed by WA-3 (MissedRetrieval) at the session level. The broader cross-session detection layer remains a future opportunity.

### Domain-Specific GNN Pretraining

Once W3-1 ships and deployment data accumulates, pretrained domain-specific weight vectors (SRE, legal, environmental monitoring) could be published as config-file starting points, dramatically reducing the cold-start period for new deployments in known domains.

### W3-2: Knowledge Synthesis

See Wave 3 section. Deferred pending W3-1 deployment and knowledge base density assessment.
