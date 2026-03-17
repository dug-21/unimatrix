# ASS-022/01: Domain Agnosticism Assessment — Current State

**Date**: 2026-03-16
**Type**: Technical deep assessment
**Builds on**: ASS-009 (2026-02-24) — cross-domain portability analysis

---

## 1. What Has Changed Since ASS-009

ASS-009 concluded that Unimatrix's architecture was "90% ready for multi-domain deployment" with coupling confined to four server-level constants. That conclusion was sound. But six weeks of shipping have added surface area to evaluate.

### New features shipped since ASS-009:

| Feature | Domain Coupling Impact |
|---------|----------------------|
| **crt-019**: Empirical Bayesian priors, adaptive `w_conf` spread | Neutral — both are domain-agnostic improvements to the confidence engine |
| **col-022**: `context_cycle` tool (feature cycle start/stop with keywords) | **New coupling** — the tool's framing ("feature cycles") and schema entry for `keywords` bake software-development workflow language into a first-class MCP tool |
| **nan-005**: Documentation & README rewrite | Neutral — changes how Unimatrix presents itself, not what it is |
| **bugfix-264/280/294**: Server performance + report decoupling | Neutral — internal plumbing |
| **21-rule observation pipeline** (fully operational) | **Existing coupling confirmed** — metrics like `bash_for_search_count`, `coordinator_respawn_count`, `sleep_workaround_count` are Claude Code-specific |
| **crt-018**: Knowledge effectiveness analysis | Neutral — measures helpfulness signals without domain assumptions |

**Updated verdict**: Still 90% domain-agnostic, but `context_cycle` has materially deepened the workflow coupling at the MCP interface layer. The tool name and framing are the most visible non-generic surface in the entire API.

---

## 2. Coupling Heat Map

Rating each component: 🟢 Generic | 🟡 Lightly coupled | 🔴 Workflow-specific

### Storage Layer (`unimatrix-store`)
```
EntryRecord schema ................................ 🟢 Generic
  - topic, category, tags, content, title, source .. 🟢 Free-form strings
  - status enum (Active/Deprecated/Proposed/Q) ..... 🟢 Universal lifecycle states
  - confidence f64 ................................. 🟢 Generic quality signal
  - content_hash, previous_hash (SHA-256 chain) .... 🟢 Universal integrity
  - trust_source (String) .......................... 🟡 Values "agent"/"neural"/"auto" are dev-flavored
  - feature_cycle (String) ......................... 🟡 Name betrays dev origins; type is free-form
  - helpful_count, unhelpful_count ................. 🟢 Universal feedback mechanism
  - access_count, last_accessed_at ................. 🟢 Universal usage tracking
  - supersedes / superseded_by ..................... 🟢 Universal correction chain model
  - correction_count ............................... 🟢 Universal quality signal

SQLite schema tables .............................. 🟢 Generic (25 fields, all purpose-neutral)
AGENT_REGISTRY / AUDIT_LOG ....................... 🟢 Generic access control
CO_ACCESS ......................................... 🟢 Universal co-occurrence tracking
OUTCOME_INDEX ..................................... 🟢 Generic outcome attribution
```

### Intelligence Layer (`unimatrix-vector`, `unimatrix-embed`)
```
HNSW vector index ................................ 🟢 Domain-agnostic by design
DistDot metric (cosine on L2-norm) ............... 🟢 Universal similarity metric
Embedding pipeline (ONNX + tokenizers) ........... 🟡 Text-only; assumes text input
384-dim Sentence Transformers model ............... 🟡 General English text; not domain-specialized
EmbedConfig model selection ...................... 🟢 7 configurable models, swappable
```

### Confidence Scoring (`unimatrix-server/src/infra/confidence.rs`)
```
W_USAGE = 0.16 (access frequency, log-transformed) . 🟢 Generic
W_HELP = 0.12 (Bayesian helpfulness voting) ......... 🟡 Assumes human raters; less applicable to sensor data
W_CORR = 0.14 (correction chain quality) ............ 🟢 Generic
W_TRUST = 0.16 (trust source tier) .................. 🟡 Values named for dev workflow
W_FRESH = 0.18 (exponential time decay) ............. 🔴 168h half-life hardcoded (1 week) — dev-centric
W_BASE = 0.16 (status + trust_source) ............... 🟡 trust_source values are dev-flavored

Co-access affinity (+0.03 max) .................... 🟢 Generic co-occurrence boost
Provenance boost for "lesson-learned" (+0.02) ..... 🔴 Hardcodes a dev-domain category name
Adaptive w_conf spread ........................... 🟢 Generic statistical adaptation
```

### MCP Interface (`unimatrix-server/src/mcp/tools.rs`)
```
context_search ..................................... 🟢 Generic semantic search
context_lookup ..................................... 🟢 Generic deterministic query
context_store ...................................... 🟢 Generic knowledge storage
context_get ........................................ 🟢 Generic by-ID retrieval
context_correct .................................... 🟢 Generic correction chains
context_deprecate .................................. 🟢 Generic deprecation
context_quarantine ................................. 🟢 Generic quarantine/restore
context_status ..................................... 🟢 Generic health/stats
context_briefing ................................... 🟡 Role/task framing is agent-workflow oriented
context_enroll ..................................... 🟢 Generic agent registration
context_retrospective ............................. 🟡 "Feature cycle" framing; logic is generic
context_cycle ...................................... 🔴 Tool name, param names, docs all dev-specific
```

### Observation Pipeline (`unimatrix-server/src/infra/observations.rs`)
```
HookType enum ..................................... 🔴 {PreToolUse, PostToolUse, SubagentStart, SubagentStop} = Claude Code-specific
UniversalMetrics struct .......................... 🔴 bash_for_search_count, sleep_workaround_count, coordinator_respawn_count
21 detection rules ............................... 🔴 All framed around Claude Code session anti-patterns
```

### Category Allowlist (`unimatrix-server/src/infra/categories.rs`)
```
CategoryAllowlist runtime extensibility .......... 🟢 Any string accepted
Initial 8 categories ............................. 🟡 Dev-domain defaults; easily replaced
"lesson-learned" as special cased category ....... 🔴 Hardcoded in confidence scoring (provenance boost)
"duties" as categories for context_briefing ...... 🟡 Generic concept, dev-workflow naming
```

### Server Instructions (`SERVER_INSTRUCTIONS` const)
```
Language: "Before starting implementation, architecture, or design tasks..." 🔴 Dev-specific
```

---

## 3. The Four Hard Coupling Points

There are exactly four things that actually tie Unimatrix to agentic software development:

### 3.1 The Freshness Half-Life (168h)

**Location**: `unimatrix-server/src/infra/confidence.rs`

```rust
const FRESHNESS_HALF_LIFE_HOURS: f64 = 168.0; // 1 week
```

This is the most consequential domain-specific parameter. A one-week decay is appropriate for software development knowledge — patterns change, decisions get revised, recent conversations are more relevant. But:

| Domain | Appropriate half-life | Ratio vs. current |
|--------|----------------------|-------------------|
| Air quality IOC readings | 1–4 hours | 0.006× |
| Software dev conventions | 1 week (168h) | 1.0× (baseline) |
| SRE runbooks | 1–3 months | 4–12× |
| Legal statutes | 1–5 years | 50–260× |
| Medical protocols (stable guidelines) | 6 months – 2 years | 100–700× |
| Scientific methods (foundational) | 5–20 years | 250–1000× |

For air quality sensor readings, knowledge about a specific pollution event is MOST valuable in the first hour and near-worthless by next week. For medical protocols, the opposite is true — a 168h half-life would decay well-established guidelines to near-zero confidence within a month of last access.

**This is not a weight issue — it is a dimensional mismatch.** The freshness decay rate must be configurable per deployment.

### 3.2 The "lesson-learned" Provenance Boost

**Location**: `unimatrix-server/src/services/search.rs`

```rust
if record.category == "lesson-learned" {
    combined += LESSON_LEARNED_BOOST; // 0.02
}
```

This hardcodes a domain-specific category name into the scoring pipeline. The concept (boosting entries that represent hard-won insights from failure) is universal. The category name is not.

In other domains:
- SRE: would be `post-mortem`
- Research lab: would be `troubleshooting` or `failed-hypothesis`
- Air quality: would be `anomaly-finding` or `sensor-failure-record`
- Legal: would be `case-reversal` or `distinguishing-authority`

The fix is to make the "boosted categories" list configurable, not hardcoded.

### 3.3 The `context_cycle` Tool

**Location**: `unimatrix-server/src/mcp/tools.rs`

The tool exists to mark the start/stop of "feature cycles" — an explicitly software development concept. The underlying functionality (grouping knowledge by a lifecycle label with keyword metadata) is generic. But:

- The tool name (`context_cycle`) implies cyclical software delivery
- The parameter name (`feature_cycle`) appears throughout the schema
- The description speaks of "feature cycles" (singular dev concept)

In other domains this maps to: monitoring campaigns (env), clinical trial phases (pharma), experimental runs (research), incident periods (SRE), legislative sessions (legal). The field should be renamed `lifecycle_label` or `context_tag` and the tool made more generic.

### 3.4 The Observation Pipeline HookTypes

**Location**: `unimatrix-server/src/infra/observations.rs`

```rust
pub enum HookType {
    PreToolUse,
    PostToolUse,
    SubagentStart,
    SubagentStop,
}
```

These map exactly to Claude Code's lifecycle hook events. The observation pipeline that feeds the learning/retrospective features assumes Claude Code as the event source. To extend to other domains, the event schema would need to be generalized (or made pluggable).

For non-agentic domains (sensor networks, batch processes, API-driven systems), there is no concept of "pre tool use." The learning pipeline would need a different event schema:
- Environmental: `SensorReadingIngested`, `QualityCheckFailed`, `CalibrationApplied`, `AnomalyDetected`
- Pharma: `TrialPhaseCompleted`, `SafetySignalFlagged`, `DataDatabaseLocked`, `RegulatorySurveillance`
- SRE: `IncidentOpened`, `RunbookApplied`, `IncidentClosed`, `PostMortemCompleted`

---

## 4. What Does NOT Need to Change

This is equally important. The following are genuinely domain-agnostic and should not be touched for multi-domain deployment:

1. **The `EntryRecord` schema** — All 25 fields work in every domain. `feature_cycle` is poorly named but functionally generic (it's a free-form string).

2. **The correction chain model** (`supersedes` / `superseded_by` / `correction_count`) — Knowledge correction is universal. Sensor recalibration, legal statute amendments, protocol revisions all follow the same pattern.

3. **The trust level hierarchy** (System > Privileged > Internal > Restricted) — Maps to any multi-actor system with authority tiers.

4. **The co-access tracking** — When two pieces of knowledge are retrieved together, that co-occurrence is meaningful in any domain.

5. **The HNSW vector index** — Completely indifferent to what's embedded. Replace text vectors with sensor signature vectors, protein embeddings, or code AST embeddings; the index doesn't care.

6. **The confidence composite structure** (6 factors, adaptive re-ranking) — The STRUCTURE is correct. The specific default values and hardcoded names need to become configurable.

7. **The audit log** (immutable, cryptographic integrity) — Universally valuable for regulated domains.

8. **The CategoryAllowlist** (`add_category(String)`) — Already runtime-extensible. Only the initial set needs to move to config.

9. **The MCP server transport** — Transport-agnostic (stdio today, HTTP tomorrow). The 12 tools' core CRUD semantics work in any domain.

---

## 5. Effort to Full Domain Agnosticism

Building on ASS-009's Option D estimate, with updated scope from what has actually shipped:

| Change | Where | Effort | Priority |
|--------|-------|--------|----------|
| Externalize freshness half-life | `confidence.rs` | 1h | **Critical** |
| Externalize boosted categories list | `search.rs` | 1h | **Critical** |
| Externalize initial category allowlist | `categories.rs` | 2h | High |
| Externalize server instructions | `server.rs` | 1h | High |
| Generalize `context_cycle` tool name/params | `tools.rs` | 4h | Medium |
| Rename `feature_cycle` in EntryRecord | Schema + migration | 4h | Low (breaking change) |
| Generalize HookType enum | `observations.rs` | 1-2 days | Low (observation-only) |
| Externalize default agent bootstrap | `registry.rs` | 2h | Medium |
| Externalize content scanning rules | `scanning.rs` | 2h | Medium |

**Critical path to multi-domain deployment: ~6-8 hours of code changes** (half-life, boosted categories, category allowlist, server instructions). The rest can be deferred. The schema rename is optional — `feature_cycle` works as a generic lifecycle label even if the name is awkward.

---

## 6. The Confidence Weights Question

The user specifically asked: *"are the weights we've adjusted recently are too heavily scoped to a agentic development tool?"*

**Short answer: The weight distribution is defensible for general use, but three specific values embed dev-workflow assumptions.**

### Weight Distribution Analysis

The current distribution: `base(0.16) + usage(0.16) + freshness(0.18) + help(0.12) + correction(0.14) + trust(0.16) = 0.92`

Freshness being the largest single factor (0.18) is a reasonable general-purpose choice — most knowledge systems benefit from recency. Usage being equal to base/trust (0.16 each) reflects that peer validation (usage) is as important as provenance (trust). Correction quality having meaningful weight (0.14) reflects that evolved knowledge is better than static knowledge.

**This distribution is sound for any "living knowledge base" that is actively used.**

### What's actually dev-specific in the confidence system:

| Factor | Issue | Severity |
|--------|-------|----------|
| W_FRESH (0.18) | The *parameter* (168h half-life), not the weight | **High** — dimensional mismatch for slow-decay domains |
| W_HELP (0.12) | Assumes human raters available; N/A for automated pipelines | **Medium** — safe default, just inapplicable without raters |
| W_TRUST (0.16) | Values "agent", "neural", "auto" are dev-workflow vocabulary | **Low** — concept is generic; vocabulary is renaming only |
| Provenance boost | Hardcodes "lesson-learned" category name | **High** — breaks if that category doesn't exist |
| W_BASE (0.16) | trust_source="auto" = 0.35; AI-generated material is penalized | **Low** — policy decision, arguably correct for any domain |

### What's genuinely good across domains:

The **adaptive `w_conf` spread** (crt-019) is an excellent generic design. The server observes the actual confidence distribution and adjusts how much weight semantic similarity vs. stored confidence gets at query time. This is domain-agnostic signal processing — it works correctly whether the knowledge base has high variance (sensor data) or low variance (reference material).

The **Bayesian helpfulness priors** (crt-019) are similarly robust. Cold-start priors (3.0, 3.0) → empirically estimated from voted entries. This degrades gracefully in domains where explicit voting is rare.

**Conclusion**: The weights themselves (0.16/0.16/0.18/0.12/0.14/0.16) are not the problem. The problem is that the freshness *parameter* and the provenance boost *category name* are hardcoded for one domain. Fix those two things and the confidence system is genuinely general-purpose.
