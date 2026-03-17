# Unimatrix — Product Vision & Roadmap

---

## Vision

Unimatrix is a self-learning knowledge integrity engine. It captures knowledge that emerges from doing work — in any domain — and makes it trustworthy, correctable, and ever-improving. It delivers the right knowledge at the right time.

---

## Story

Unimatrix began in agentic software delivery, where the problem was specific: AI agents forget, contradict each other, and confidently repeat mistakes. We built a knowledge engine where nothing is merely stored — everything is attributed, hash-chained for integrity, scored by real usage, and correctable with full provenance. Agents stopped relitigating decisions. Knowledge started improving with every delivery.

That foundation became a platform. A typed knowledge graph formalizes relationships — not just what agents retrieve together, but why: support, contradiction, supersession, dependency. A confidence system learns from actual usage rather than manual calibration, adapting weights and decay rates to each domain's signal patterns. Contradiction detection is semantic. Any event source — hooks, webhooks, automated pipelines — feeds the learning layer without agent cooperation. Any knowledge-intensive domain — environmental monitoring, SRE operations, scientific research, regulatory compliance — runs on the same engine, configured not rebuilt. Secured with OAuth, containerized, serving any number of repositories from a single instance. The integrity chain runs through all of it: hash-chained corrections, immutable audit log, trust-attributed provenance — tamper-evident from first write to last.

---

## The Critical Gaps

Before the roadmap, a clear-eyed list of where Unimatrix has strayed from
its domain-agnostic foundations, and where new surface area has been accumulated:

### Domain Coupling (strayed from)
| Gap | Severity | Location |
|-----|----------|----------|
| Freshness half-life hardcoded at 168h (1 week) | Critical | `confidence.rs` |
| "lesson-learned" category name hardcoded in scoring | Critical | `search.rs` |
| Lambda dimension weights hardcoded (freshness 0.35, graph 0.30, contradiction 0.20, embedding 0.15) | Critical | `confidence.rs` |
| SERVER_INSTRUCTIONS const uses dev-workflow language | High | `server.rs` |
| Initial category allowlist hardcoded (8 dev categories) | High | `categories.rs` |
| `context_retrospective` tool name is SDLC-specific ("retrospective" is Agile vocabulary) | Medium | `tools.rs` |
| `context_cycle` parameter labels dev-specific (feature, sprint) — tool concept is domain-neutral | Low | `tools.rs` |
| HookType enum tied to Claude Code events | Medium | `observations.rs` |
| trust_source vocabulary dev-flavored ("agent","neural","auto") | Low | `confidence.rs` |
| Observation metrics schema (bash_for_search, coordinator_respawn) | Low | `observations.rs` |

**Note on `context_cycle`**: The tool concept is domain-neutral — start/stop of any bounded work
unit applies equally to a sprint, an incident, a measurement campaign, or a legal case. What
is dev-specific is the parameter vocabulary ("feature", "sprint"). W0-3 config externalizes
those labels; the tool itself does not need to change.

**Note on `context_retrospective` → `context_cycle_review`**: "Retrospective" is Agile/Scrum
vocabulary. The rename to `context_cycle_review` is domain-neutral ("review" applies to any
cycle — post-incident review, campaign review, case review) and makes the pairing with
`context_cycle` self-evident: you start/stop a cycle, then review it. This rename is
a W0-3 scope addition — low-effort, high clarity gain.

**Note on `feature_cycle` and `topic`**: These fields are domain-neutral free-form strings.
`topic` is what the knowledge is *about*; `feature_cycle` is what *work context produced* it.
Non-dev deployments should map their domain equivalent (incident ID, campaign ID, case number)
to these fields — neither is dev-specific in function, only in naming convention.
Domain pack documentation must make this explicit to prevent operators from leaving
`feature_cycle` unwired.

### Security (needs upgrade)
| Gap | Severity |
|-----|----------|
| Auto-enroll gives read access to any unknown process | High |
| agent_id per-call model: friction, unreliable, spoofable | High |
| No token-based client identity for STDIO | High |
| No path to OAuth for centralized deployment | Medium |

### Scalability & Architecture
| Gap | Severity |
|-----|----------|
| Process exits on session end — background tick, write queue, ML inference stop | Critical |
| Single SQLite writer — MCP requests compete with all background work | High |
| No backup/recovery story — SQLite lives in project directory | High |
| No container deployment model | Medium |
| No HTTP transport — stdio only | Medium |
| Graph rebuilt at query time from snapshot, not persisted | Medium |

### Intelligence & Confidence
| Gap | Severity |
|-----|----------|
| Confidence weights hardcoded — cannot adapt to domain or usage | High |
| Only supersession edge type — no typed relationships | High |
| Contradiction detection uses cosine heuristic, not NLI | Medium |
| Graph edges not persisted — lost on restart | Medium |
| Co-access and contradiction never formalized as graph edges | Medium |

---

## Wave 0 — Prerequisites (do first, unblock everything)
*Estimated: ~1.5 weeks*

These are not features. They are the structural preconditions that everything
else depends on. None changes external behavior.

### W0-0: Daemon Mode ← **do this first**
**What**: Transform Unimatrix from a per-session stdio process into a persistent
background daemon that survives MCP client disconnection.

- `unimatrix serve --daemon` starts the server as a long-lived process
- Listens on a **Unix Domain Socket** (UDS) in the project data directory
- Claude Code connects via UDS instead of spawning a new stdio process per session
- When a session ends (stdin closes / client disconnects), the daemon keeps running
- PidGuard + flock already provides one-daemon-per-project enforcement (vnc-004)
- Auto-start: if no daemon is running when a client connects, spawn one

**Why first**: Every Wave 1+ intelligence feature assumes continuous background
processing — write queue draining, NLI post-store inference, tick-based cache
rebuilds, GNN training, GGUF overnight synthesis. All of these are meaningless
if the process exits at session end. Without daemon mode, Wave 1 delivers the
infrastructure for background intelligence but the background never actually runs
between sessions.

**Why UDS not HTTP**: Keeps it local. No network exposure, no TLS management,
no auth surface beyond file-system permissions. The daemon serves any number of
dev workspace sessions from a single process. HTTP transport (W2-2) is an
additive layer on top of an already-working daemon — not the first time background
processing works.

**Operational scope for now**: dev workspace only. Full container packaging
and HTTP exposure are Wave 2. The daemon validates background processing
end-to-end before containerization adds operational complexity.

**Effort**: 2-3 days (UDS transport, client reconnect handling, auto-start logic).

**Security requirements:**
- [High] UDS socket file must be created with `0600` permissions (owner-only);
  group or world-readable sockets allow any local process to connect without authentication.
- [Medium] The auto-start path must not re-use a stale PID file from a crashed daemon —
  verify the PID is live before concluding a daemon is already running; use the existing
  `is_unimatrix_process` cmdline check (vnc-004).
- [Low] The `CallerId::UdsSession` exemption from rate limiting (already established)
  applies to UDS connections — document that this exemption is local-only and must
  never extend to HTTP transport callers.

---

### W0-1: Two-Database Split
**What**: Separate the single `unimatrix.db` into `knowledge.db` (integrity chain)
and `analytics.db` (learning layer). Each database gets its own independent
SQLite connection. The analytics write queue consumer **owns its connection directly**
— no `Mutex<Connection>`, no `spawn_blocking` for analytics writes.

```
knowledge.db  → entries, entry_tags, vector_map, counters,
                agent_registry, audit_log,
                sessions*,         ← see durability note below
                feature_entries*   ← see durability note below

analytics.db  → co_access, graph_edges (new), observation_metrics,
                injection_log, signal_queue, query_log,
                shadow_evaluations, outcome_index,
                topic_deliveries, confidence_weights (new)
```

**Write queue architecture**: The analytics write queue is a bounded async channel.
The queue consumer is a dedicated async task that owns the `analytics.db` connection
and drains the channel directly — no `spawn_blocking`, no `Mutex`. Simple SQL writes
are sub-millisecond and safe to run on the async task. The queue must have an explicit
bounded depth with a defined shed-under-load policy (drop with logging) to prevent
unbounded growth under burst signal pressure.

**Durability decision required**: `sessions` and `feature_entries` are causal
preconditions for `knowledge.db` writes — implicit vote attribution (crt-020) depends
on session records, and feature cycle lifecycle attribution depends on feature_entries.
"Eventually consistent" is the wrong durability model for these tables. Two options:

- Option A (recommended): Keep `sessions` and `feature_entries` in `knowledge.db`.
  These are low-volume tables; the single-writer overhead is acceptable.
- Option B: Create a "durable analytics" connection with synchronous writes,
  separate from the async-queue `analytics.db` connection.

Resolve this decision before implementation begins.

**Why first**: Every subsequent addition writes to `analytics.db`. Without this split,
each new background worker competes with MCP request writes. With it, MCP hot path
writes to `knowledge.db` with zero contention — forever. The two-DB split relaxes
write contention but **does not** address CPU-bound ML inference pressure on the
`spawn_blocking` pool — that is addressed separately in W1-2.

**Integrity**: `knowledge.db` is the trust-critical file. Back up frequently.
`analytics.db` is the learning layer — eventually consistent, self-healing on restart.
A crash in analytics never affects the integrity chain. However, all analytics-derived
data used on the search hot path (graph edges, confidence weights) must be cached
in memory (`Arc<RwLock<_>>` rebuilt by tick) — the search hot path must never read
`analytics.db` directly at query time.

**Effort**: 3-4 days (connection routing, write queue, durability decision).

**Security requirements:**
- [High] Enforce separate file-system permissions on `knowledge.db` vs `analytics.db` —
  `knowledge.db` should be owner-read-write only (0600); `analytics.db` can be
  group-readable. Compromise of analytics data must not yield the integrity chain.
- [High] The async write queue draining to `analytics.db` must authenticate writes —
  queue entries must carry the originating `agent_id` and a monotonic sequence number
  so that queue replay attacks cannot inject un-attributed edges or observation events.
- [Medium] Cross-database consistency invariant: if an entry is deprecated in
  `knowledge.db` and the analytics write queue is mid-flight with a co-access or
  confidence update for that entry, the queue consumer must tolerate and discard stale
  writes without error — validate target entry existence before applying analytics updates.
- [Low] No foreign key enforcement exists across the DB boundary; document that
  `analytics.db` referential integrity is eventually-consistent and must never be
  trusted for security decisions (only `knowledge.db` is authoritative).

---

### W0-2: Session Identity via Env Var — **tracked by GH #293**
**What**: Replace `PERMISSIVE_AUTO_ENROLL = true` with env-var-configured session identity.

- `UNIMATRIX_SESSION_AGENT` in `settings.json` → default agent_id for all tool calls
- Auto-enroll the session agent at startup with capabilities defined in W0-3 config
  `[agents] bootstrap` — not hardcoded as `[Read, Write, Search]`
- `PERMISSIVE_AUTO_ENROLL` converted from compile-time const to env-var (default `false`)
- Unknown/unconfigured callers get `[Read, Search]` only — no Write without a configured identity
- `agent_id` per-call still works for audit attribution; identity no longer needs to be passed on every call
- Forward-compatible: `UNIMATRIX_SESSION_AGENT` slot is replaced by OAuth token claims when HTTP transport arrives

**Reconcile with ADR #1839**: ADR #1839 (UNIMATRIX_CLIENT_TOKEN for STDIO) addresses
token-based client identity for the same problem. Resolve whether W0-2 supersedes,
extends, or layers on top of #1839 before implementation. Two diverging env-var
identity mechanisms must not ship simultaneously.

**Scope note**: Capability defaults for a configured session agent belong in W0-3 config
(`[agents] bootstrap`) not hardcoded here — different domains need different defaults.
`[Read, Write, Search]` is appropriate for the dev LLM workflow but should not be
a compile-time assumption.

**Why first**: Eliminates the `agent_id: "human"` spoofing problem and makes the
capability system non-vestigial. Required before any multi-user or HTTP exposure.

**Effort**: Scoped in GH #293.

**Security requirements:**
- [Critical] `UNIMATRIX_SESSION_AGENT` must not fall back to a privileged default if
  unset — the unset case must produce `[Read, Search]` only, never `[Read, Write, Search]`.
  Writing without explicit identity configuration must be a deliberate opt-in.
- [High] Env var value must be validated as a non-empty string matching
  `[a-zA-Z0-9_-]{1,64}` before use as `agent_id`; reject and refuse startup if the
  value would produce an agent_id that spoofs a protected agent (`system`, `human`).
- [High] When `PERMISSIVE_AUTO_ENROLL=false` (new default), the rejection path for
  unknown callers must return a structured error — not silently fall back to a weaker
  identity — so that misconfigured clients fail loudly rather than operating with
  reduced privileges invisibly.
- [Medium] The env var is set in `settings.json` (committed to the repository); treat
  `UNIMATRIX_SESSION_AGENT` as a non-secret identifier, not a credential. Document that
  it provides attribution only — it is not an authentication factor and must not be
  relied upon as one until HTTP + OAuth lands in W2-3.

---

### W0-3: Config Externalization
**What**: Move hardcoded constants to `~/.unimatrix/config.toml` (or per-project):

```toml
[knowledge]
categories = ["outcome", "lesson-learned", "decision", "convention",
              "pattern", "procedure", "duties", "reference"]
boosted_categories = ["lesson-learned"]   # previously hardcoded in search.rs
freshness_half_life_hours = 168.0         # previously hardcoded in confidence.rs

[confidence]
# Lambda dimension weights — previously hardcoded in confidence.rs
# Operators MUST set these for non-dev domains (legal: high trust/low fresh;
# air-quality: high fresh/low trust). Dev-domain defaults are shown below.
weights = { freshness = 0.35, graph = 0.30, contradiction = 0.20, embedding = 0.15 }

[server]
instructions = """..."""                  # previously SERVER_INSTRUCTIONS const

[agents]
default_trust = "restricted"
bootstrap = [
  { id = "system", trust = "system",     capabilities = ["Admin", "Write", "Read", "Search"] },
  { id = "human",  trust = "privileged", capabilities = ["Admin", "Write", "Read", "Search"] },
]
# Session agent default capabilities (used by W0-2):
session_capabilities = ["Read", "Write", "Search"]

[cycle]
# context_cycle tool parameter labels — rename for non-dev domains
work_context_label = "feature"   # label shown in tool descriptions
cycle_label = "cycle"
# context_retrospective is renamed context_cycle_review:
# "retrospective" is Agile vocabulary; "review" is domain-neutral
```

**Why first**: This is the single unlock for domain agnosticism. Every other
domain-coupling gap either disappears (hardcoded category names, freshness rate,
confidence weights, instructions) or becomes trivially fixable via config.

**Lambda weights are now included**: The confidence dimension weights
(freshness 0.35, graph 0.30, contradiction 0.20, embedding 0.15) are domain-specific
constants. A legal knowledge base needs high `w_trust`, low `w_fresh`. An air quality
deployment needs the inverse. Externalizing these is the interim fix that bridges
the gap until W3-1's GNN learns them automatically. Without this, the confidence
system remains domain-coupled through all of Wave 1 and 2.

**`context_cycle` parameter labels**: The tool's dev-specific vocabulary
("feature", "cycle", "sprint") is now configurable. Non-dev domain packs set
labels appropriate to their domain without changing tool logic.

**Domain packs** become possible immediately after: an SRE deployment sets different
categories, different instructions, different freshness rates, different confidence
weights. Same binary.

**Effort**: 1-2 days (schema grows slightly from original estimate).

**Security requirements:**
- [Critical] `config.toml` `[server] instructions` is loaded verbatim into the MCP
  server's system-level tool description — it is a direct prompt injection surface.
  Validate and sanitize on load: strip or reject content matching the existing injection
  pattern set (~50 patterns in `ContentScanner`). Refuse server startup if instructions
  contain `ignore previous`, `you are now`, or other override triggers.
- [High] `config.toml` file permissions must be validated at startup: reject
  world-writable config (`mode & 0o002 != 0`) and log a warning if group-writable.
  A compromised config can silently reconfigure category allowlists and server
  instructions — both are trust-critical inputs.
- [High] The `categories` allowlist is the sole gate preventing unknown category values
  from entering the knowledge base. After externalization, validate all category values
  in config against a max-length (64 chars), character allowlist (`[a-z0-9_-]`), and
  a reasonable count ceiling (≤ 64 categories). Reject config load on violation.
- [Medium] `boosted_categories` values must be a strict subset of `categories`; validate
  this invariant at load time and fail startup on mismatch rather than silently boosting
  an uncategorized label.
- [Medium] Confidence weights must sum to ≤ 1.0 and each weight must be in [0.0, 1.0];
  reject config on violation to prevent adversarial weight configurations that distort
  all confidence scores.

---

## Wave 1 — Intelligence Foundation
*Estimated: 2 weeks, after Wave 0*

### W1-1: Typed Relationship Graph
**What**: Upgrade `StableGraph<u64, ()>` to `StableGraph<u64, RelationEdge>`.
Persist edges to `GRAPH_EDGES` in `analytics.db`. Bootstrap from existing data.

```rust
// Store RelationType as a string in analytics.db — NOT an integer discriminant.
// Integer encoding locks extensibility: adding a new type requires schema migration
// AND GNN retraining (W3-1 feature vector changes). String encoding allows extension
// without either.
enum RelationType { Supersedes, Contradicts, Supports, CoAccess, Prerequisite }
struct RelationEdge { relation_type: String, weight: f32, created_at, created_by, source }
```

On startup: populate `Supersedes` from `entries.supersedes`, `Contradicts` from
`shadow_evaluations`, `CoAccess` from high-count `co_access` pairs.

**In-memory cache model (required)**: The typed graph, like the existing petgraph
in-memory cache (crt-014 fix), must follow the `Arc<RwLock<_>>` tick-rebuild pattern.
The search hot path reads from the in-memory graph only — never from `analytics.db`
directly at query time. The tick rebuilds the in-memory graph from the persisted
`GRAPH_EDGES` after compaction completes. This insulates search from analytics.db
consistency windows.

**Bootstrap edge status**: Edges bootstrapped from `shadow_evaluations`
(cosine-similarity heuristics known to produce false positives) must carry
`source: "bootstrap"` and a `bootstrap_only: true` flag. Bootstrap edges are
**excluded from confidence scoring** until W1-2 NLI either confirms (promotes to
`source: "nli"`) or refutes (marks for deletion) them. Injecting unconfirmed
contradiction edges into scoring from day one would penalize valid entries.

**ADR-004 supersession required**: ADR #1604 (topology-derived penalty scoring)
assumes `()` edge weights. W1-1 upgrades to `weight: f32`. ADR #1604 must be
explicitly superseded with a new ADR before W1-1 ships or the penalty computation
is architecturally inconsistent.

**Compaction sequencing**: During maintenance tick, `GRAPH_EDGES` cleanup
(orphaned edges from deprecated entries) and `VECTOR_MAP` compaction must both
complete *before* the tick triggers an in-memory rebuild. Sequence within the tick;
never run concurrently. The in-memory rebuild always sees a post-compaction consistent
state from both databases.

**Why now**: Foundation for NLI (W1-2) and GNN (Wave 3). Also unlocks free
DOT/GraphViz export (petgraph supports it). Formalizes what the system already
knows but never persists. Edges survive restarts.

**Integrity**: Every edge write goes through the analytics write queue with
`created_by` attribution. Edge creation is audited. Graph is a *view* of the
integrity chain, not a separate truth.

**Effort**: 3-4 days.

**Security requirements:**
- [High] Every `RelationEdge` write must carry `created_by` attribution and be
  routed through the analytics write queue — not written directly. An unauthenticated
  edge insertion path would allow privilege-adjacent manipulation (e.g., injecting a
  `Contradicts` edge to suppress a valid entry via confidence penalty).
- [Medium] Edge `weight` values are `f32` — validate that they are finite (not NaN or
  ±Inf) before persisting; a NaN weight propagated into confidence scoring can corrupt
  search rankings silently.
- [Medium] Bootstrap edges carry `source: "bootstrap"` and are excluded from scoring
  until confirmed by NLI — do not apply confidence penalties from bootstrap-only
  contradiction edges.
- [Low] DOT/GraphViz export must sanitize entry content embedded as node labels —
  entry titles may contain characters that break DOT syntax or inject
  visualization-layer payloads.

---

### W1-2: Embedded NLI Model + Rayon Thread Pool
**What**: One small ONNX model (~180MB, DeBERTa-v3-small fine-tuned on NLI)
for real contradiction and support detection. **This wave also establishes the
rayon thread pool pattern for all CPU-bound ML inference.**

- Runs **post-store**, async — not on search hot path
- Input: (entry A content, entry B content) for top-K nearest neighbors of new entry
- Output: {entailment, neutral, contradiction} scores
- On contradiction > threshold: create `Contradicts` edge via analytics write queue
- On entailment > threshold: create `Supports` edge
- Replaces cosine-similarity heuristic in `shadow_evaluations`

**Thread pool architecture (do at W1-2, not later)**:

CPU-bound ML inference must not run in `spawn_blocking`. The documented pool
saturation incidents (#735, #1628, #1688) all stem from CPU-bound or long-duration
work consuming the tokio blocking pool. NLI inference (50-200ms), and all future
ML inference (GNN in W3-1, GGUF in W3-3), runs on a **dedicated `rayon::ThreadPool`**:

```rust
// Bridge rayon → tokio async:
let (tx, rx) = tokio::sync::oneshot::channel();
rayon_pool.spawn(move || {
    let result = run_nli_inference(input);
    tx.send(result).ok();
});
let result = rx.await?;  // async task suspends; zero tokio threads consumed
```

**At W1-2, also migrate the existing ONNX embedding model to rayon.** Embedding
inference is 10-50ms of CPU work — above the `spawn_blocking` threshold. Migrating
embedding at W1-2 (lower stakes, already proven) validates the rayon pattern before
NLI depends on it. `spawn_blocking` then handles only short I/O-bound operations
(DB writes, file reads). All ML inference is on rayon.

**Circuit breaker on NLI → auto-quarantine path**: NLI creates `Contradicts` edges
→ topology penalty activates (ADR #1604 successor) → entries may fall below
auto-quarantine threshold (crt-018b). This feedback loop must have a rate limit:
cap the number of `Contradicts` edges created per tick. NLI-derived auto-quarantine
should require a higher confidence threshold than the existing manual-correction path.

**Graceful degradation**: If the NLI model file is absent, hash-invalid, or fails
to load, the server must start successfully and fall back to the cosine-similarity
heuristic (current behavior) with a logged warning. NLI absence must not prevent
startup. Follow the same pattern as the embedding model (ADR-005 graceful degradation).

**Domain qualifier**: NLI (trained on SNLI/MultiNLI) performs well on natural-language-
dense knowledge bases. For structured or domain-specific corpora (sensor calibration
records, terse SRE runbooks, numeric regulatory text), the cosine heuristic may
perform comparably or better. Do not present NLI as universally superior — qualify
it by domain knowledge density.

**Why now**: Contradiction detection is one of the most cited differentiators.
The current heuristic produces false positives (similar topic, compatible content)
and false negatives (same topic, genuinely conflicting). NLI fixes both for
natural-language-dense domains. NLI scores become edge quality features for W3-1.

**Effort**: 3-4 days (model integration + rayon pool establishment + embedding migration).

**Security requirements:**
- [Critical] ONNX model integrity must be verified at load time via SHA-256 hash
  pinned in config — a replaced model file is an undetectable model-poisoning attack
  vector. Refuse startup if hash does not match.
- [High] NLI input pairs are derived from stored knowledge entries — content stored
  by external callers. Run each input through length truncation (max 512 tokens /
  ~2000 chars per side) before inference to prevent adversarial inputs that cause
  ONNX runtime out-of-memory or extreme inference latency.
- [High] NLI inference runs on rayon; if it panics (OOM, malformed tensor), the panic
  must not propagate to the MCP handler thread. The oneshot channel drop signals
  failure cleanly to the awaiting async task.
- [Medium] The `Contradicts`/`Supports` edge creation thresholds must be config-driven
  (W0-3) — these thresholds directly affect which knowledge entries are penalized.

---

### W1-3: Observation Pipeline Generalization
**What**: Make `HookType` pluggable rather than Claude Code-specific.

```rust
// Instead of:
enum HookType { PreToolUse, PostToolUse, SubagentStart, SubagentStop }

// A registered event schema:
struct ObservationEvent {
    event_type: String,    // "pre_tool_use", "sensor_reading", "anomaly_detected"
    source_domain: String, // "claude-code", "ndp", "custom"
    payload: JsonValue,
    session_id: String,
}
```

**Domain pack registration is config-file-driven, not runtime MCP calls.**
Domain packs register their event types and extraction rules via a TOML file loaded
at startup — not via a runtime Admin `context_enroll` call. Runtime registration
adds an Admin privilege barrier that prevents self-configuring deployments.
Startup config registration is reproducible, version-controllable, and consistent
with W0-3's config model. Runtime re-registration (for dynamic reconfiguration)
remains available to Admin callers as an override.

The `UniversalMetrics` schema similarly becomes configurable — dev-specific metrics
(`bash_for_search`, `coordinator_respawn`) become the default domain pack's metrics,
not hardcoded struct fields.

**Detection rule rewrite is in scope for W1-3.** The 21 retrospective detection
rules (col-002) reference `HookType` variants and `UniversalMetrics` field names
structurally. Generalizing the event schema without simultaneously rewriting the
detection rules breaks the retrospective pipeline for any non-Claude-Code domain.
The detection rule rewrite is not optional — it is part of W1-3's deliverable.
W3-1's implicit training labels depend on a functioning retrospective pipeline;
broken detection rules break the GNN training signal.

**Why now**: Required for any non-Claude-Code raw signal source (NDP events,
SRE incidents, sensor anomalies). Also feeds the GNN training signal pipeline (Wave 3).

**Effort**: 5-7 days (touches observation pipeline, detection rules, metrics schema,
domain pack config loading — detection rule rewrite adds to original estimate).

**Security requirements:**
- [High] Domain pack event type registration must be restricted to Admin capability
  for runtime calls — any agent able to register event types can define extraction
  rules that read arbitrary fields from `payload: JsonValue`.
- [High] `payload: JsonValue` is fully untyped external input; enforce maximum payload
  size (64KB) and depth limit (nesting ≤ 10 levels) to prevent DoS.
- [Medium] Domain pack extraction rules must be sandboxed — pure data transformation,
  no side effects, no environment variable or filesystem references.
- [Medium] `source_domain` must be normalized and validated (max 64 chars,
  `[a-z0-9_-]`) before use as an observation attribution label.

---

## Wave 2 — Deployment
*Estimated: 2 weeks, after Wave 0*
*(Can run in parallel with Wave 1)*

### W2-1: Container Packaging
**What**: Dockerfile + docker-compose for single-binary container with named volumes.

```
unimatrix-knowledge volume:   ← back up frequently; integrity-critical
  knowledge.db
unimatrix-analytics volume:   ← back up less frequently; self-healing
  analytics.db
unimatrix-shared volume:
  models/            ← ONNX weights (re-downloadable, not critical)
  config.toml        ← mount as read-only bind mount (see security)
```

**Separate volumes for knowledge.db and analytics.db**: They have different backup
cadences and different criticality. A single volume snapshot backs up both at the same
frequency, making differential backup inoperable. Separate volumes allow the operator
to snapshot `unimatrix-knowledge` frequently and `unimatrix-analytics` less often.

**Container lifecycle**: Include a `HEALTHCHECK` that verifies the daemon is alive
and the schema version is current. Without a health check, container orchestrators
cannot determine readiness. For the STDIO/UDS daemon, use a process-level check
or a minimal UDS probe.

Container is stateless except the volumes. Backup = volume snapshot of
`unimatrix-knowledge`. Works locally (alongside dev containers) or in cloud
(EBS, Cloud Persistent Disk).

**Solves**: The backup/recovery problem. SQLite no longer lives in project directories.
Point-in-time recovery becomes standard container ops.

**Effort**: 2 days.

**Security requirements:**
- [High] Each named volume must be set to owner-only at container build time
  (`chmod 0700 /data/knowledge`, `chmod 0700 /data/analytics`); world-readable
  volumes expose both databases to any host process with volume access.
- [Medium] Mount `config.toml` as a read-only bind mount from a secrets manager or
  CI/CD vault rather than storing it in a data volume. Config-in-volume conflates
  runtime state with configuration and makes version control of config harder.
- [Low] Container must run as a non-root user (`USER unimatrix` in Dockerfile);
  validate that both databases are created successfully under non-root ownership.

---

### W2-2: HTTP Transport + Basic Auth
**What**: Add HTTP/HTTPS transport alongside existing UDS/stdio.
The MCP tools are unchanged — only the transport layer differs.

```
unimatrix serve --transport uds    # daemon mode (W0-0), default
unimatrix serve --transport stdio  # legacy stdio, unchanged
unimatrix serve --transport http   # new, adds HTTP + token auth middleware
             --port 8080
             --tls-cert /certs/cert.pem
             --tls-key  /certs/key.pem
```

HTTP transport: validates `Authorization: Bearer <token>` header against
AGENT_REGISTRY. Capability check is identical — same service layer, different
transport resolution path.

**Verify rmcp HTTP transport readiness before committing to estimate.** The claim
"rmcp supports Streamable HTTP transport natively" must be verified against rmcp 0.16's
actual implementation maturity before scoping this as 3-4 days. If gaps exist, the
estimate expands and the Wave 2 schedule shifts.

**Dual transport**: A deployment that wants to serve both Claude Code (UDS) and
external systems (HTTP) simultaneously from a single process must be explicitly
designed. The daemon (W0-0) already handles multiple UDS clients. HTTP is an
additive transport on the same engine instance — both must be able to run
concurrently against the same store.

**Why now**: Required for the centralized (1 platform, N repositories) model.
Also enables NDP and other external systems to call Unimatrix without being
Claude Code plugins.

**Effort**: 3-4 days (contingent on rmcp HTTP transport verification).

**Security requirements:**
- [Critical] TLS is non-negotiable for HTTP transport — do not ship an `--insecure`
  or `--no-tls` flag. If TLS cert/key paths are not provided at startup, refuse to
  start in HTTP mode.
- [Critical] Bearer token validation must be constant-time comparison.
- [High] Enforce: maximum request body size (≤ 1MB), connection timeout (30s),
  maximum concurrent connections at the HTTP server layer.
- [High] No unauthenticated health/metrics endpoints on the same port.
- [Medium] Add IP-level rate limiting as a secondary defense layer for HTTP callers.

---

### W2-3: Multi-Project Routing + OAuth Middleware
**What**: Two-tier knowledge hierarchy and OAuth 2.0 client credentials flow.

**Two-tier store routing** (personal collection and project isolation):

```
Owner Store  (owner knowledge.db + analytics.db)
  └── ProjectStore × N  (per-project knowledge.db + analytics.db each)
```

At query time, search fans out to the project store (highest specificity, highest
weight) and the owner store (shared conventions and patterns). Write always goes
to the project store. Promotion from project to owner tier is explicit, attributed,
and hash-chained — a new entry created at the owner level with provenance back to
the source project entry.

This is valuable even at personal scale: conventions and lessons that apply across
all your projects accumulate in the owner store rather than being rediscovered
per project. For teams, the owner store becomes the organization tier. A global
tier (platform-curated domain packs) is deferred.

**This adds a routing layer — "no changes to storage" is incorrect.** W2-3 requires
a `TenantRouter` that resolves the correct `Arc<Store>` pair (project + owner) at
request time. Tool logic is unchanged; the store resolution layer above it is new.
Design the `TenantRouter` abstraction to accommodate future tiers (global) without
a rewrite.

**OAuth scopes map to Unimatrix capabilities:**

```
unimatrix:search → Search
unimatrix:read   → Read
unimatrix:write  → Write
unimatrix:admin  → Admin
```

`sub` claim → `agent_id` for audit attribution. `unimatrix_project` custom claim
→ project database routing.

**`context_enroll` dual-use clarification**: Enrolling a knowledge workflow agent
(a principal with a trust level) and registering an OAuth client (a service identity
with scopes) are different operations with different lifecycle requirements (OAuth
clients rotate secrets; agents do not). These should be distinct operations or at
minimum clearly distinguished in the API, not conflated in a single tool call.

**Effort**: 4-5 days (two-tier routing layer + OAuth middleware; larger than original
"middleware only" estimate due to TenantRouter).

**Security requirements:**
- [Critical] JWT validation must enforce: algorithm allowlist (`RS256`/`ES256` only),
  expiry (`exp`), issuer (`iss`), and audience (`aud`). Missing `aud` check is OWASP A07.
- [Critical] `sub` claim → `agent_id` must be validated against `[a-zA-Z0-9_-]{1,64}`.
- [High] `unimatrix_project` claim for routing — path traversal risk. Validate against
  a strict allowlist of registered project identifiers; never construct file paths
  directly from claim values.
- [High] OAuth client secrets must never be stored in `knowledge.db` or `analytics.db`.
  `context_enroll` stores only `client_id`, never `client_secret`.
- [Medium] Establish maximum token TTL policy (≤ 1 hour).

---

## Wave 3 — Adaptive Intelligence
*Estimated: 3-4 weeks, after Wave 1 + sufficient usage data*

### W3-1: GNN Confidence Learning
**What**: Replace hardcoded weight constants with a learned weight vector
per knowledge base. A small Graph Attention Network (2-layer, ~400KB ONNX)
trains on helpfulness signals and behavioral patterns from the observation pipeline.

**Inputs**: Per-node features (6 raw factor scores, category, trust, graph structural
features: degree, chain depth, contradiction neighbor count). Per-edge features:
RelationType, NLI confidence score, co-access count.

**Outputs**: `[w_base, w_usage, w_fresh, w_help, w_corr, w_trust]` learned weight
vector (sum = 0.92) + learned `freshness_half_life_hours`.

**In-memory weight cache (required)**: Learned confidence weights live in `analytics.db`
(`confidence_weights` table) but are consumed on the search hot path. Follow the same
in-memory tick-cache pattern as the graph — load weights into memory at startup and
after each training run. The search hot path reads from memory only; `analytics.db`
is the persistence layer. A missing or stale weight vector degrades gracefully to
the config-defined defaults (W0-3 `[confidence] weights`).

**Config-driven cold-start weights (W0-3 prerequisite)**: W3-1's cold-start
initializes from the weights in `[confidence] weights` config, not hardcoded
dev-domain constants. Operators of non-dev domains set domain-appropriate starting
weights in config so cold-start is reasonable from day one, not after weeks of
convergence. The GNN then refines from there.

**Training loop architecture must be specified before implementation**:
- When does training run? (maintenance tick recommendation; define resource envelope)
- Batch retraining vs. incremental updates?
- How long does a training run take at expected knowledge base sizes?
- How does training interact with other tick work (compaction, confidence refresh)?
- Runs on rayon `ThreadPool` (consistent with all ML inference from W1-2 onward)

**Training signal — dual source**:
1. *Explicit*: `helpful_count`/`unhelpful_count` per entry
2. *Implicit*: observation pipeline behavioral patterns —
   retrieval → successful completion = positive; retrieval → re-search = negative;
   retrieval → error pattern = negative. Requires W1-3 detection rules to be fully
   functional for the generalized event schema.

**Gate condition**: Deploy when a knowledge base has 50+ helpfulness votes OR
sufficient observation pipeline events (typically 2-4 weeks of active daemon use
under W0-0).

**Effort**: 1-2 weeks (no GNN infrastructure exists in the codebase; unimatrix-learn
has MLP/EWC/MicroLoRA but not graph attention networks — effort estimate needs
validation against actual implementation complexity).

**Security requirements:**
- [High] GNN training data integrity: enforce Wilson score minimum-vote guard (already
  at 5 votes) and per-agent vote-rate limit (max 10 helpfulness votes per agent per
  hour) before GNN is deployed.
- [High] Implicit training labels must be attributed to sessions, not individual
  agent_ids, to prevent synthetic label injection.
- [Medium] Learned weight vector must be stored with a checksum and training run
  input hash to detect tampering between runs.
- [Medium] Cold-start from config-defined weights (not hardcoded dev-domain defaults)
  must be documented as the operator's responsibility to configure for their domain.

---

### W3-2: Knowledge Synthesis
**What**: Maintenance-tick process that distills knowledge clusters into
single synthesized entries.

**W3-3 (GGUF module) is a hard prerequisite for W3-2.** Without an LLM, synthesis
is concatenation with a label — a multi-entry blob stored as `trust_source="neural"`
that appears authoritative but contains no actual distillation. Promoting concatenated
content to neural trust level degrades the knowledge base by creating entries that
look synthesized but aren't. W3-2 must not ship until W3-3 is deployed and the
synthesis path produces genuine LLM-distilled output.

Trigger: 3+ Active entries, same topic+category, mutual Supports/CoAccess edges,
combined content > `synthesis_token_threshold` (config-driven, default 800 tokens),
no existing synthesis entry for cluster, cluster cardinality ≤ `max_cluster_size`
(config-driven, default 20 entries — prevents unbounded synthesis on high-cardinality
sensor or event topics).

Output: synthesized entry with `trust_source="neural"`, confidence = GNN-weighted
average of sources, `supersedes` = lowest-confidence source. Source entries deprecated
(not deleted), correction-chained to synthesis.

**Gate condition**: Deploy when knowledge base exceeds ~200 clustered entries
on any topic AND W3-3 is deployed. Premature synthesis at low entry counts produces
noise; synthesis without LLM produces deceptive concatenations.

**Effort**: 1 week (after W3-3).

**Security requirements:**
- [High] Synthesized entries must validate that all source entries are Active and not
  quarantined before inclusion; a quarantined source blocks synthesis of that cluster.
- [High] The `supersedes` chain from synthesized entry to source entries must go
  through the same hash-chaining and audit-log path as manual corrections.
- [Medium] Apply full content scanner (`S1`) to synthesized content before storing —
  treat neural output as external input.
- [Low] Gate condition uses a stable count snapshot from the start of the maintenance
  tick, not a live query.

---

### W3-3: Optional Inference Module (GGUF)
**What**: An optional `unimatrix-infer` capability — a local GGUF model loaded via
llama.cpp when configured. Enabled by a single config entry; absent means zero impact
on existing behavior.

```toml
[inference]
model_path = "/absolute/path/to/models/phi-3.5-mini.Q4_K_M.gguf"
# absent = graceful degradation; all existing behavior unchanged
```

**All GGUF inference runs on the rayon `ThreadPool`** (established in W1-2).
"Dedicated thread budget" means a bounded rayon pool sized separately from the
NLI inference pool — GGUF inference is longer-duration (seconds, not milliseconds)
and must not starve NLI inference during overnight synthesis runs.

**llama.cpp integration complexity**: llama.cpp via Rust FFI introduces a third ML
stack alongside the two existing ONNX pipelines. Platform-specific compilation
(ARM, x86, macOS, Linux), signal handler conflicts, and memory management differences
from short-lived processes are known risks in long-running server processes. The
"1 week" estimate should be validated against a proof-of-concept integration before
committing to the timeline. Build the GGUF integration behind a Cargo feature flag
(`features = ["infer"]`) so it doesn't affect builds for deployments that don't need it.

**Synthesis atomicity**: Overnight GGUF synthesis runs may be interrupted by daemon
restart. The synthesis path must be atomic at the commitment point — a "synthesis in
progress" state that survives restart without leaving orphaned partial entries. Do not
commit a synthesized entry until the full LLM output is available and hash-chained.

**When present, qualitatively upgrades:**

- **context_retrospective** — 21 detection rules produce pattern-matched findings today.
  With LLM: genuinely reasoned recommendations. *"Three sessions this week showed the same
  re-search pattern on integration test setup. The convention entry is technically correct
  but missing the fixture initialization step every agent has had to rediscover."*
  That is not a rule. That is reasoning.

- **context_status recommendations** — heuristic thresholds → specific, actionable,
  contextual explanations of why health is degraded and what to do.

- **W3-2 Knowledge Synthesis** — with LLM: genuine distillation of multiple entries
  into a coherent, attributable, higher-quality single entry.

- **Contradiction explanation** — NLI gives a score; LLM gives the *why*.

- **Background intelligence without an external LLM** — With daemon mode (W0-0),
  retrospectives, synthesis, and contradiction analysis run overnight, ready when the
  next session begins — without Claude.

**Why optional rather than required**: Unimatrix without the module is fully functional.
All capabilities degrade gracefully to current behavior. The module is an enhancement
tier, not a dependency. W3-2 synthesis, however, requires W3-3.

**Recommended models by deployment:**

| Deployment | Model | Size (Q4) | Notes |
|-----------|-------|-----------|-------|
| Raspberry Pi 5 (16GB) | Llama-3.2-1B | ~800MB | Sufficient for synthesis + recommendations |
| Developer laptop | Phi-3.5-mini | ~2.2GB | Better reasoning quality |
| Any deployment | Any GGUF-format model | — | User-supplied; Unimatrix provides the integration |

**Reference deployment**: Raspberry Pi 5 (16GB) running neural-data-platform with no
external LLM. Unimatrix + GGUF module transforms the Pi from reactive alerting
(sensor exceeds threshold → alert) to a reasoning system (anomaly detected →
knowledge search → LLM synthesis: *"This PM2.5 signature matches three prior events
correlated with westerly winds during wildfire season — check sensor 4 for drift"*).
Fully air-gapped. Zero cloud dependency. The Pi reasons about its own sensor data.

**Gate condition**: User opt-in via config. No minimum data threshold. Validation
of the llama.cpp integration in the target deployment environment (ARM vs x86,
available RAM) before enabling in production.

**Effort**: 1-2 weeks (proof-of-concept integration required before committing).

**Security requirements:**
- [Critical] GGUF model file must be hash-verified at load — same SHA-256 pinning
  pattern as W1-2 (NLI). A replaced model file is an undetectable prompt-injection
  and reasoning-manipulation vector.
- [High] LLM input is composed from stored knowledge entries. Apply length limits
  (max ~4000 tokens total context) and run inputs through the content scanner before
  passing to the model.
- [High] LLM output stored as synthesized entries or returned as recommendations must
  pass full content scanner before storage or return.
- [Medium] `model_path` must be an absolute path validated against an allowed directory
  prefix — prevent misuse of the GGUF loader's file read capability.
- [Low] Run GGUF inference exclusively on the dedicated rayon pool with a bounded
  thread budget to prevent monopolizing CPU during long synthesis operations.

---

## Dependency Graph

```
W0-0: Daemon mode (UDS, persistent) ─────────────────────────────────────────┐
                                                                               ▼
Wave 0 (prerequisites — W0-1/2/3 in parallel, after W0-0)
  W0-1: Two-DB split ──────────────────────────────────────────────────────┐
  W0-2: Client token security ─────────────────────────────────────────────┤
  W0-3: Config externalization (incl. confidence weights, cycle labels) ───┤
                                                                             │
        ┌────────────────────────────────────────────────────────────────────┤
        ▼                                                                     ▼
Wave 1 (intelligence foundation)                       Wave 2 (deployment)
  W1-1: Typed graph ────────────────┐                  W2-1: Container
  W1-2: NLI model + rayon pool ─────┤                  W2-2: HTTP transport ──┐
  W1-3: Obs generalization ─────────┤                  W2-3: Multi-project    │
        (incl. detection rule        │                        + OAuth ◄────────┘
         rewrite)                    │
                                     │
        ┌────────────────────────────┘
        ▼
Wave 3 (adaptive intelligence — after daemon + usage data accumulates)
  W3-3: GGUF module    (independent; prerequisite for W3-2)
  W3-1: GNN learning   (needs W1-1, W1-3 signals + usage data from W0-0 daemon)
  W3-2: Synthesis      (needs W3-1 weights + W3-3 LLM + entry density)
```

Waves 1 and 2 are fully independent — run in parallel after Wave 0.
W3-3 is independent and should be deployed before W3-1/W3-2.
Wave 3 cannot start until Wave 1 is complete AND sufficient usage data exists
(requires W0-0 daemon running continuously to accumulate signal).

---

## Effort Summary

| Wave | Items | Estimated Effort | Gate Condition |
|------|-------|-----------------|----------------|
| W0-0 | Daemon mode | ~3 days | — (do first) |
| W0 | 3 items | ~1.5 weeks | W0-0 complete |
| W1 | 3 items | ~2.5 weeks | W0 complete (detection rule rewrite adds to W1-3) |
| W2 | 3 items | ~2 weeks | W0 complete (parallel with W1; rmcp HTTP verification needed) |
| W3-3 | GGUF module | ~1-2 weeks | User opt-in; proof-of-concept validation first |
| W3-1 | GNN learning | ~1-2 weeks | W1 complete + usage data; training loop design first |
| W3-2 | Synthesis | ~1 week | W3-1 + W3-3 both deployed |

**Total to domain-agnostic, securely deployed, scalable platform**: ~6-7 weeks of focused work.
Wave 3 trails by however long it takes for daemon usage data to accumulate (weeks to months).

---

## What's Preserved Throughout

Every wave maintains these non-negotiables:

- **Hash chain integrity**: `content_hash` / `previous_hash` on every entry — untouched by any wave
- **Correction chain model**: `supersedes`/`superseded_by` — extended by W1-1 but not modified
- **Immutable audit log**: every operation attributed and logged — W0-2 strengthens this
- **ACID storage**: SQLite transactional guarantees — W0-1 splits but doesn't weaken
- **Single binary**: all waves add capability to the same binary, not new services
- **Zero infrastructure**: container is optional, not required; daemon + UDS works without it
- **In-memory hot path**: all analytics-derived search data (graph, weights, co-access)
  cached in `Arc<RwLock<_>>` rebuilt by tick — `analytics.db` never read directly at query time

The integrity chain is the product's defensible moat. The roadmap is designed
around it, not in spite of it.

---

## What This Unlocks

After W0-0 (daemon):
- Background tick, write queue, ML inference, and GNN training run continuously
- "Overnight intelligence" (W3-2/W3-3) becomes possible — not container-only

After Wave 0 + W1 + W2:
- Any domain can deploy with a config file (SRE, environmental, research, legal)
- Raw signals from any source (Claude Code hooks, NDP events, custom) feed the learning layer
- Contradiction detection is semantically grounded, not heuristic
- Typed relationships are first-class, persistent, attributed
- Container deployment with clean backup/recovery
- HTTP endpoint for external integrations (NDP → Unimatrix)
- Multi-project routing — your collection of projects shares a common knowledge tier
- OAuth-gated access for team deployments

After Wave 3:
- Confidence weights adapt to each domain's actual usage patterns automatically
- The freshness hardcoding problem dissolves — learned per deployment
- Knowledge clusters self-compress as they mature
- A new domain gets config-defined starting weights from day one and GNN-learned
  weights within weeks of active daemon use
- With W3-3 (GGUF module): retrospective and status recommendations become genuinely
  intelligent; synthesis produces coherent distillations; contradiction explanation
  surfaces the *why*; the system reasons about its own knowledge without an external LLM
- Reference: a Raspberry Pi 5 running neural-data-platform, fully air-gapped,
  becomes a self-contained intelligent sensor platform

---

## Security Cross-Cutting Concerns

### Threat Model Evolution

**Wave 0 — daemon-local (hardened)**

Threat actors are local processes on the same machine, now connecting to a persistent
UDS socket rather than spawning a new process per session. The daemon changes the
attack surface slightly: the UDS socket is a persistent connection endpoint.

Primary risks:
- A process claiming any `agent_id` it wants (spoofing) — W0-2 closes this partially
- UDS socket accessible to any local user — file permissions (0600) gate access
- Config files writable by any local user (config injection) — W0-3 must enforce file permissions
- Auto-enroll granting write access to any unknown process — W0-2 closes this with `PERMISSIVE_AUTO_ENROLL=false`

Blast radius: one machine, one knowledge base. Recovery = restore from backup.

**Wave 1 — daemon-local with ML inference**

New threat actors: adversarial knowledge inputs designed to manipulate NLI scoring
or GNN training. The knowledge base itself becomes an attack surface.

Risks:
- Vote manipulation to corrupt GNN labels (W3-1 risk, enabled by W1 infrastructure)
- Adversarial entry pairs crafted to maximize false `Supports` edges
- Model file replacement between ONNX integrity checks
- NLI → auto-quarantine feedback loop without circuit breaker

Blast radius: corrupted confidence weights affect every query result. Recovery =
retrain GNN from clean observation snapshot + restore knowledge.db from backup.

**Wave 2 — HTTP-exposed**

The server becomes network-accessible. Standard network threat actors apply:
- Token theft and replay (requires TLS)
- Credential stuffing against the token validation endpoint
- Request amplification / slow-read DoS
- SSRF if any tool can be made to fetch external resources (Unimatrix currently cannot — maintain this)

Blast radius: network-wide exposure of all knowledge content and audit history.
Recovery = token revocation + audit review.

**Wave 3 — multi-project (personal collection, then team)**

Multiple projects share one owner-tier knowledge base via token claims and store routing.
Threat actors include misconfigured project routing:
- Project claim manipulation to access another project's knowledge base (path traversal)
- Cross-project observation data leakage via shared analytics infrastructure
- Owner tier pollution from a compromised project's promoted entries

Blast radius: cross-project data exposure. Recovery = per-project key rotation + audit review.

---

### Non-Negotiables Across All Waves

**1. Hash chain integrity is immutable.**
`content_hash` and `previous_hash` on every entry must never be skipped, backdated,
or made optional. This includes synthesized entries (W3-2), auto-extracted entries,
and any entry produced by background maintenance. Every write path is a hash chain write.

**2. Audit log is append-only and complete.**
Every operation that changes the state of `knowledge.db` — store, correct, deprecate,
quarantine, enroll, synthesize — must produce an AUDIT_LOG entry with `agent_id`,
`session_id`, `operation`, `target_ids`, and `outcome`. No write path bypasses the audit.
The analytics write queue (W0-1) must not become an audit bypass for analytics-side writes.

**3. Capability checks are enforced at the service layer, not the transport layer.**
Whether the caller arrives via UDS, stdio, HTTP bearer token, or OAuth JWT,
the capability check (`Admin`, `Write`, `Read`, `Search`) happens in the service layer
after identity resolution. Transport-layer authentication is a precondition, not a substitute.

**4. Content scanning is not bypassed for machine-generated content.**
`AuditSource::Internal` bypasses S1 content scanning (by design, for performance).
This bypass must only apply to content generated by the Unimatrix process itself
(confidence updates, usage increments, observation recording). It must never apply to:
- Content synthesized from stored entries (W3-2) — treat as external
- NLI model outputs stored as edge labels — treat as external
- Config-derived content (server instructions) stored as knowledge — treat as external

**5. No secret material in `knowledge.db` or `analytics.db`.**
OAuth client secrets, API keys, TLS private keys, and any other credentials must
never be stored in either database. If `context_enroll` is extended to support OAuth
client registration, store only `client_id` — never `client_secret`.

**6. The UDS session exemption from rate limiting remains local-only.**
`CallerId::UdsSession` is exempt from rate limiting because it represents local traffic.
This exemption must never extend to HTTP transport callers. Rate limiting for HTTP
callers must be enforced without exception.

**7. Analytics-derived data is never read directly on the search hot path.**
`analytics.db` is eventually consistent. All analytics-derived data used during search
(graph edges, confidence weights, co-access affinities) must be cached in memory and
rebuilt by tick. Direct `analytics.db` reads at query time are an availability risk
and must be treated as an architectural violation.

---

### Architectural Decisions Required Before Wave 2

**Decision 1: Token format and validation strategy for HTTP bearer tokens**

Option A: Opaque tokens stored in AGENT_REGISTRY (lookup-based validation).
Option B: Signed JWTs validated locally without DB lookup.

Recommendation: Option A for W2-2. Reserve JWT for W2-3 (OAuth).
The critical requirement: validation must be constant-time at the comparison step.

**Decision 2: TLS termination responsibility**

Option A: Unimatrix terminates TLS directly.
Option B: TLS terminated by a reverse proxy; Unimatrix sees plaintext HTTP.

Recommendation: Support both. If Option B, server must bind to `127.0.0.1` only —
not `0.0.0.0`. Enforce this in the `--transport http` startup checks.

**Decision 3: Multi-project isolation model**

Per-project `knowledge.db` + `analytics.db` for all tiers. Shared `analytics.db`
across projects is a cross-project observation leakage risk — session patterns, query
logs, and topic attributions from one project would appear in another project's
retrospectives. Per-project for both is the only safe model.

**Decision 4: W0-1 sessions/feature_entries durability**

Resolve before W0-1 implementation: move `sessions` and `feature_entries` to
`knowledge.db` (Option A, recommended) or create a synchronous "durable analytics"
connection (Option B). These tables are causal preconditions for `knowledge.db`
writes and cannot tolerate eventual consistency.

---

## Future Opportunities

These are not roadmap items — they require no new waves and do not block anything.
Each is additive and could be picked up after the roadmap waves complete.

### Proactive Knowledge Discovery

**What**: `context_cycle_review` and `context_status` already analyze session evidence
and have store access at call time. Extending them to produce structured `KnowledgeCandidate`
records closes a gap in the feedback loop: today the system is entirely reactive (agents
must decide to store). This adds a detection layer where the analysis tools surface
topics that *should* have entries but don't.

**Signal sources** (no new infrastructure required):
- `query_log` in `analytics.db`: recurring queries with low top-similarity (< 0.4 across
  3+ sessions) — the gap is already proven by the search evidence, no re-search needed
- Re-derivation sequences in `context_cycle_review`: agent searched → low results →
  succeeded anyway → no `context_store` issued — pattern indicates missing knowledge
- Co-access clusters in `analytics.db` with no synthesizing entry

**Architecture**:
- `candidate_topics` table in `analytics.db` accumulates signals across analysis runs
- `context_cycle_review` and `context_status` produce a `candidates` section alongside
  existing output when signal strength exceeds threshold (e.g., 3+ sessions)
- Candidates are never auto-stored — they require an explicit `context_store` call from
  a human or privileged agent; the integrity chain requires attributed intent
- With W3-3 (GGUF): the review tool passes candidate + session evidence to the local LLM
  and returns a draft entry; without GGUF, it surfaces the topic and evidence only

**Effort**: Detection layer ~2 days (extends W1-3 retrospective pipeline + analytics schema).
Drafting layer ~2-3 days added to W3-3. Fully additive — no wave restructuring required.
