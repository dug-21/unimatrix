# D14-3: Existing Feature Impact Assessment

**ASS-014 Research Spike — RQ-3**
**Date:** 2026-03-01
**Status:** Complete

---

## Executive Summary

The cortical implant introduces a second access path (hook-driven, automatic) alongside the existing MCP path (agent-initiated, explicit). This dual-path architecture impacts seven completed features to varying degrees. The most significant transformation is in **col-002 (Retrospective Pipeline)**, where the JSONL telemetry pipeline can be substantially simplified because the implant provides structured events directly rather than requiring file-system JSONL parsing. **crt-001 (Usage Tracking)** and **crt-004 (Co-Access Boosting)** require schema-level accommodation of a new `source` dimension to distinguish MCP retrievals from hook injections. **crt-002 (Confidence Evolution)** needs differentiated weighting for implicit bulk signals (from col-009) versus per-retrieval explicit signals. **col-001 (Outcome Tracking)** gains complementary auto-generated outcomes from session lifecycle but retains agent-authored outcomes as the high-fidelity path. **vnc-003 (context_briefing)** is the closest analog to compaction defense but requires a new session-aware query interface to serve PreCompact hooks. None of these changes block col-006 implementation — all can proceed incrementally after the transport layer ships.

---

## RQ-3a: col-002 (Retrospective Pipeline)

### Current Implementation

The `unimatrix-observe` crate (`crates/unimatrix-observe/src/`) provides a complete observation-driven retrospective pipeline:

- **JSONL Parser** (`parser.rs`): Reads session-scoped `.jsonl` files from `~/.unimatrix/observation/`. Parses `RawRecord` structs with fields `{ts, hook, session_id, tool, input, response_size, response_snippet}`. Normalizes SubagentStart/Stop fields. Skips malformed lines gracefully. Sorts by timestamp.
- **Feature Attribution** (`attribution.rs`): Content-based sequential scanning — walks records in timestamp order, identifies feature switch points via file paths (`product/features/{id}/`), task subjects, and git checkout patterns. No hardcoded prefix allowlist (fixed in #59). Pre-feature records attributed to first feature found.
- **Hotspot Detection** (`detection/mod.rs` + 4 submodules): 21 rules across 4 categories — Agent (7: context load, lifespan, file breadth, re-reads, mutation spread, compile cycles, edit bloat), Friction (4: permission retries, sleep workarounds, search-via-bash, output parsing struggle), Session (5: timeout, cold restart, coordinator respawns, post-completion work, rework events), Scope (5: source file count, design artifacts, ADR count, post-delivery issues, phase duration outliers). Each rule implements the `DetectionRule` trait.
- **Metric Computation** (`metrics.rs`): `compute_metric_vector()` produces a `MetricVector` (21 universal metrics + per-phase duration/tool counts) from observation records and hotspot findings.
- **Report Assembly** (`report.rs`): `build_report()` creates a `RetrospectiveReport` with session count, record count, metrics, hotspots, and optional baseline comparison.
- **Session File Management** (`files.rs`): Discovers `.jsonl` files, computes stats, identifies expired files (60-day cleanup). Files live at `~/.unimatrix/observation/`.
- **Storage**: `OBSERVATION_METRICS` table (14th table in redb) stores MetricVector per feature_cycle as bincode bytes.

The pipeline is invoked via `context_retrospective` (12th MCP tool). Shell hooks (`product/research/ass-011/hooks/observe.sh`) capture PostToolUse events into per-session JSONL spool files.

### Impact from Cortical Implant

The cortical implant absorbs the observation hook role. Instead of a shell script appending JSONL lines to spool files, the implant captures events directly as structured data:

1. **JSONL file path eliminated for live sessions**: The implant holds event data in memory (daemon mode) or writes structured events to Unimatrix directly. The filesystem spool (`~/.unimatrix/observation/spool/*.jsonl`) becomes unnecessary for implant-mediated sessions.

2. **Feature attribution simplifies**: The implant knows the current feature context from session metadata (col-010 SessionStart hook provides `feature_cycle`). Content-based sequential scanning in `attribution.rs` — which today must infer feature from file paths and task subjects — can be replaced with an explicit feature tag on each event.

3. **Session boundary becomes explicit**: Currently, session boundaries are inferred from JSONL file names and timestamp gaps (cold restart detection uses 30-minute gap threshold). With col-010, SessionStart/SessionEnd hooks provide explicit boundaries.

4. **Observation records gain richer data**: The implant sees all hook types (UserPromptSubmit, PreCompact, TaskCompleted, etc.), not just PreToolUse/PostToolUse/SubagentStart/SubagentStop. This enables new detection rules without additional shell hooks.

### Simplification Opportunities

| Component | Current | With Implant | Savings |
|-----------|---------|-------------|---------|
| `parser.rs` (195 LOC) | Parses JSONL from filesystem | Receives structured `ObservationRecord` directly | Can eliminate JSONL parsing for implant path; retain for legacy/offline analysis |
| `attribution.rs` (137 LOC) | Content-based feature inference | Feature tag provided by session context | ~90% of attribution logic becomes fallback-only |
| `files.rs` (124 LOC) | Session file discovery, cleanup | Implant manages session lifecycle | Reduced to archive/offline analysis |
| `observe.sh` hook (47 lines) | Shell script writing JSONL | Implant IS the hook handler | Eliminated entirely |
| Cold restart detection | 30-minute gap heuristic | Explicit SessionStart/End signals | Heuristic becomes unnecessary |

**Net: ~400 LOC in `unimatrix-observe` shifts from "primary path" to "legacy/offline fallback."**

### Migration Path

1. **Phase 1 (col-006 ships)**: Implant dispatches observation events. `unimatrix-observe` receives them via a new `from_structured_events()` entry point (bypasses JSONL parsing). Feature attribution uses implant-provided feature tag. JSONL path retained as fallback.
2. **Phase 2 (col-010 ships)**: Explicit session lifecycle replaces file-based session discovery. `files.rs` session management becomes cleanup-only.
3. **Phase 3 (stabilization)**: JSONL path deprecated. `parser.rs` and `files.rs` moved to a `compat` module. Detection rules and metric computation remain unchanged — they operate on `ObservationRecord` regardless of source.

### Recommendation

**Extend, then migrate** — Add a structured event ingestion path to `unimatrix-observe` alongside the existing JSONL parser. Do not deprecate JSONL until implant adoption is confirmed stable. The detection rules, metric computation, baseline comparison, and report assembly are source-agnostic and remain as-is. The `DetectionRule` trait interface (`fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>`) requires no changes.

**Migration strategy (2026-03-01):** The cortical implant will provide ALL the data the JSONL path provides, plus richer dimensions (injection tracking, agent identity, explicit feature context, entry-level outcome correlation). The 21 detection rules and baseline comparison logic remain unchanged — they operate on `ObservationRecord` regardless of source. However, the entire ingestion pipeline (JSONL parsing, file discovery, content-based feature attribution) should eventually be replaced by the structured event path from the cortical implant:

1. **Phase 1 (col-006)**: Dual ingestion — JSONL path for backward compatibility, structured event path for implant-mediated sessions. Same `ObservationRecord` downstream.
2. **Phase 2 (col-010 stable)**: Structured path becomes primary. JSONL path becomes legacy/offline fallback. Feature attribution uses explicit session metadata instead of content scanning.
3. **Phase 3 (validation)**: JSONL path deprecated. `parser.rs`, `files.rs`, `attribution.rs` moved to `compat` module. All new detection rules built against structured event path.

**New analysis dimension:** The retrospective pipeline gains `entries_analysis` — correlating INJECTION_LOG data with session outcomes to produce entry-level performance metrics (which entries were present in successful vs. rework sessions, per-entry injection frequency, agent-specific entry effectiveness). This feeds the expanded retrospective report and the "flag-negative" confidence signal routing (entries correlated with rework surfaced for human review, not auto-downweighted).

---

## RQ-3b: col-002b (Detection & Baselines)

### Current Implementation

`col-002b` extends the retrospective pipeline with:

- **18 additional detection rules** (completing the full 21-rule library across 4 categories). Each rule implements `DetectionRule` trait and operates on `&[ObservationRecord]`.
- **Baseline computation** (`baseline.rs`): `compute_baselines()` calculates per-metric mean + stddev from historical `MetricVector` arrays. Requires minimum 3 vectors. `compare_to_baseline()` flags outliers at mean + 1.5 sigma.
- **Four arithmetic guard modes** (`BaselineStatus`): Normal, Outlier, NoVariance, NewSignal — preventing NaN/Inf in edge cases.
- **Phase-specific baselines**: Duration and tool counts computed per-phase when >= 3 data points exist.

MetricVectors are stored in `OBSERVATION_METRICS` table and retrieved for baseline comparison during retrospective analysis.

### Impact from Cortical Implant

1. **MetricVector computation input changes format, not substance**: `compute_metric_vector()` takes `&[ObservationRecord]` and `&[HotspotFinding]`. The implant changes how records arrive (structured vs. JSONL) but the MetricVector computation is identical once records are in `ObservationRecord` form.

2. **Richer metrics possible**: The implant captures events the current hooks miss (UserPromptSubmit, PreCompact, compaction frequency). New detection rules can leverage these — e.g., "compaction frequency as process signal" (SCOPE open question #9). This extends the MetricVector without changing existing metrics.

3. **Baseline comparison unaffected**: Historical MetricVectors stored in `OBSERVATION_METRICS` are compared purely on numeric values. The source of those numbers (JSONL vs. implant) is irrelevant.

4. **Phase detection simplifies**: `extract_phase_name()` in `metrics.rs` currently parses TaskCreate/TaskUpdate input JSON to find phase transitions. With col-010 session lifecycle, phase transitions can be explicit events.

### Simplification Opportunities

| Component | Current | With Implant | Impact |
|-----------|---------|-------------|--------|
| `MetricVector` schema | 21 universal + per-phase | Same + optional new fields | Additive only |
| `BaselineSet` computation | Operates on MetricVector arrays | No change | None |
| `BaselineComparison` | Numeric comparison | No change | None |
| Phase extraction | Parses TaskCreate subject field | Can use explicit phase events | Simplification |

### Migration Path

**No blocking changes.** MetricVector computation and baseline comparison operate on abstract data structures. Add new metrics to `UniversalMetrics` for implant-specific signals (e.g., `compaction_events`, `injection_count`) as new `#[serde(default)]` fields — backward compatible via zero-migration schema evolution. Existing rules and baselines continue to work during transition.

### Recommendation

**Keep as-is** — with additive extension. The baseline computation and detection rule framework are fully source-agnostic. New implant-specific detection rules can be added to `default_rules()` without modifying existing rules. The `MetricVector` schema evolves by appending new `#[serde(default)]` fields.

---

## RQ-3c: crt-001 (Usage Tracking)

### Current Implementation

Usage tracking records MCP tool retrievals via a fire-and-forget pattern:

- **`record_usage_for_entries()`** (`server.rs:476`): Called after every retrieval tool (context_search, context_lookup, context_get, context_briefing). Performs 5 steps:
  1. **Access dedup** via `UsageDedup::filter_access()` — prevents same agent from inflating `access_count` by repeatedly retrieving the same entry within a session.
  2. **Vote tracking** via `UsageDedup::check_votes()` — last-vote-wins semantics with correction support (NewVote, CorrectedVote, NoOp).
  3. **Usage recording** via `store.record_usage_with_confidence()` — increments `access_count`, `helpful_count`, `unhelpful_count` on `EntryRecord`, recomputes confidence.
  4. **Feature entry recording** via `store.record_feature_entries()` — links entries to feature cycles in `FEATURE_ENTRIES` multimap (trust-gated: System/Privileged/Internal only).
  5. **Co-access recording** (crt-004) — generates ordered pairs from retrieved entry IDs, records in `CO_ACCESS` table.

- **`UsageDedup`** (`usage_dedup.rs`): In-memory session-scoped dedup tracker using `Mutex<DedupState>`. Tracks `access_counted: HashSet<(agent_id, entry_id)>`, `vote_recorded: HashMap<(agent_id, entry_id), bool>`, `co_access_recorded: HashSet<(u64, u64)>`. Cleared on server restart.

- **`EntryRecord` fields**: `access_count: u32`, `last_accessed_at: u64`, `helpful_count: u32`, `unhelpful_count: u32`.

### Impact from Cortical Implant

Hook-injected knowledge (col-007) is "usage" but does not go through MCP tools:

1. **New usage source**: When the implant injects entries via UserPromptSubmit hook (col-007), those entries are "used" by the agent but never called via context_search/lookup/get. The current pipeline only records usage inside `record_usage_for_entries()`, which is called from MCP tool handlers.

2. **Volume increase**: Hook injection fires on every prompt (potentially many times per session), whereas MCP retrievals are infrequent. If every injection counts as "usage", `access_count` inflates rapidly, distorting usage_score in the confidence formula.

3. **Dedup model changes**: `UsageDedup` currently deduplicates per `(agent_id, entry_id)` per server session. Hook injections may not have an `agent_id` in the same sense — the implant injects on behalf of the session, not a specific agent.

4. **Helpful/unhelpful signals**: Hook injections have no explicit helpful/unhelpful parameter. The implicit signal comes from col-009 (session outcome). This is fundamentally different from per-retrieval voting.

### Design Options

**Option A: New `source` field on usage recording**
Add a `source` discriminator to usage tracking: `"mcp"` for explicit tool calls, `"hook"` for implant injections. Different dedup and counting rules per source. Hook injections count at most once per (session, entry) regardless of how many prompts injected the entry. MCP retrievals retain current behavior.

**Option B: Separate injection log**
The implant maintains its own injection log (which entries were injected, when, in which session). This feeds into the confidence pipeline (col-009) at session end rather than per-event. The `USAGE_LOG` concern from SCOPE is addressed by col-009's bulk session-end signaling, not per-injection tracking.

**Option C: Hybrid — injection tracking in implant, confidence update via session outcome**
The implant tracks injections locally (process memory or sidecar file). At session end (col-010 SessionEnd hook), it sends a batch signal to Unimatrix: "these entries were injected during this session, session outcome was X." This batch feeds `access_count` (once per session per entry) and `helpful_count`/`unhelpful_count` (based on session outcome). No per-prompt usage recording.

### Recommended Approach: Option C (Hybrid)

- **access_count**: Incremented once per entry per session when session ends (not on every injection). Prevents inflation.
- **helpful/unhelpful**: Derived from session outcome (col-009). Successful session = bulk helpful for injected entries. Failed session / rework detected = bulk unhelpful or neutral.
- **UsageDedup**: Extended with a `source` parameter. Hook injections use session-scoped dedup (not agent-scoped, since the implant has no agent identity).
- **EntryRecord**: No schema change needed. `access_count`, `helpful_count`, `unhelpful_count` fields accommodate both sources.
- **FEATURE_ENTRIES**: Hook injections can populate this multimap with the session's `feature_cycle` — the implant knows the feature context.

### Migration Path

1. **col-006 ships**: No changes to crt-001. Hook transport established but no injection yet.
2. **col-007 ships (context injection)**: Implant tracks injected entry IDs per session in process memory. No usage recording yet — injections are "free reads."
3. **col-009 ships (closed-loop confidence)**: Session-end hook sends batch usage signal. `record_usage_for_entries()` extended with `source: "hook"` variant that applies session-scoped dedup and bulk helpful/unhelpful.
4. **col-010 ships (session lifecycle)**: Clean session boundaries enable reliable "injections per session" counting.

### Recommendation

**Extend** — Add a `source` discriminator to the usage recording path and implement session-scoped batch usage recording. The existing per-retrieval MCP path remains unchanged. The `UsageDedup` struct gains a `filter_injection_access()` method that deduplicates per `(session_id, entry_id)` rather than `(agent_id, entry_id)`. No schema changes to `EntryRecord`.

---

## RQ-3d: crt-002 (Confidence Evolution)

### Current Implementation

Confidence is computed as a six-component additive weighted composite (`confidence.rs`):

```
confidence = W_BASE * base(status)       [0.18]
           + W_USAGE * usage(access)     [0.14]
           + W_FRESH * fresh(recency)    [0.18]
           + W_HELP  * help(votes)       [0.14]
           + W_CORR  * correction(chain) [0.14]
           + W_TRUST * trust(source)     [0.14]
                                          -----
                              Stored sum: 0.92
           + W_COAC (at query time)      [0.08]
                              Total:      1.00
```

Key design properties:
- All computation uses f64 (crt-005 ADR).
- Helpfulness uses Wilson score lower bound with minimum 5 votes guard.
- Usage uses log-transformed access count (caps at ~50).
- Freshness uses exponential decay with 168h half-life.
- Co-access affinity (W_COAC = 0.08) computed at query time, not stored.
- Search re-ranking: `0.85 * similarity + 0.15 * confidence + co_access_boost (max 0.03)`.

Signals flow today:
- **Explicit**: Agent passes `helpful: true/false` on context_search/lookup/get/briefing calls.
- **Per-retrieval**: Each MCP tool invocation triggers `record_usage_for_entries()` which increments access_count and helpful/unhelpful counts.
- Confidence is recomputed inside `store.record_usage_with_confidence()` using `compute_confidence()`.

### Impact from Cortical Implant

1. **Implicit bulk signals from col-009**: When a session ends, the implant infers helpfulness from the session outcome. This is fundamentally different from per-retrieval explicit voting:
   - **Volume**: A session may involve 5-20 injected entries. Bulk signaling at session end = 5-20 signals at once vs. 0-2 explicit votes per typical MCP session.
   - **Precision**: Explicit votes are targeted ("this specific entry was helpful for this specific query"). Implicit signals are correlated ("the session succeeded, so all injected entries get credit"). There is a guilt-by-association problem — if 5 entries were injected and 1 was bad, all 5 would be affected.
   - **Asymmetry required**: Positive correlation is safe (session succeeded → entries probably helped). Negative correlation is dangerous (session had rework → which of the 5 injected entries was the problem?).

2. **Wilson score minimum sample guard interaction**: The 5-vote minimum guard protects against premature confidence shifts. With bulk session-end signaling, an entry injected in one session receives 1 helpful vote. After 5 successful sessions, it crosses the threshold. This is appropriate — the guard remains effective.

3. **Attribution granularity problem**: The cortical implant cannot attribute negative outcomes to specific entries. Unlike explicit MCP votes (agent deliberately says "entry #42 was unhelpful"), implicit signals only know "the session had rework and these 5 entries were present." Auto-applying unhelpful votes to all 5 would poison good entries through guilt-by-association, degrading confidence quality over time.

### Recommended Approach: Auto-Positive, Flag-Negative, Never Auto-Downweight

**DESIGN DECISION (2026-03-01):** Implicit confidence signals are asymmetric:

- **Successful session → auto-apply Helpful**: Entries injected during successful sessions get `helpful=true` via the standard confidence pipeline. Session-scoped dedup: max 1 helpful vote per entry per session. This is safe — rising tide lifts all boats. Wilson 5-vote minimum guard prevents premature promotion.

- **Rework session → Flag, do NOT apply Unhelpful**: Entries correlated with rework are written to SIGNAL_QUEUE as `Flagged` (not `Unhelpful`). These signals are consumed by the **retrospective pipeline (col-002)**, NOT the confidence pipeline. They appear in `RetrospectiveReport.entries_analysis` as "correlated with rework" for human review alongside hotspot findings and baseline comparisons.

- **Only explicit MCP votes can downweight**: The `unhelpful_count` field is only incremented by deliberate agent action via MCP tools (`helpful=false` on context_search/lookup/get). This preserves the precision guarantee of the existing confidence system.

**Rationale:**
- Auto-positive is safe because if a session succeeded, the context didn't hurt (and probably helped). Over many sessions, statistical signal emerges naturally.
- Auto-negative is dangerous because attribution granularity is insufficient. The retrospective pipeline already has the analysis infrastructure (21 detection rules, baseline comparison, evidence collection) to surface problematic patterns for human judgment.
- This aligns with claude-flow research (#202): their confidence feedback was a no-op. The lesson is that fake or imprecise feedback creates the illusion of learning while actually degrading quality. Conservative-but-real beats aggressive-but-noisy.

**Weight handling:**
- **Same weight as explicit**: Implicit helpful votes count as regular helpful votes. No fractional weights, no separate counters. Wilson score's lower bound naturally discounts small sample sizes.
- **Session-scoped dedup**: Each entry gets at most 1 helpful vote per session from implicit signaling, regardless of injection count.

The confidence formula itself (`compute_confidence()`) requires **no changes**. It operates on `EntryRecord.helpful_count` / `unhelpful_count` regardless of signal source.

### Migration Path

1. **No changes for col-006/col-007**: Injection doesn't yet produce confidence signals.
2. **col-009 (closed-loop confidence)**: Session-end hook calls a new `record_session_outcome()` function that:
   - Takes session's injected entry IDs + session outcome (success/rework/abandoned).
   - Success → writes Helpful signals to SIGNAL_QUEUE with source=ImplicitOutcome → consumed by confidence pipeline.
   - Rework → writes Flagged signals to SIGNAL_QUEUE with source=ImplicitRework → consumed by retrospective pipeline for human review.
   - Abandoned → no signals (inconclusive).
3. **No formula changes**: `compute_confidence()` remains pure and unchanged.
4. **Retrospective evolution**: `RetrospectiveReport` gains an `entries_analysis` field that correlates INJECTION_LOG data with session outcomes, presenting entry-level performance data alongside the existing hotspot/baseline analysis.

### Recommendation

**Keep as-is** — the confidence formula requires no changes. The signal routing (auto-apply helpful, flag rework for human review) is handled in the SIGNAL_QUEUE consumer layer (col-009). The Wilson score minimum sample guard and log-transformed usage score naturally handle implicit signal characteristics. The retrospective pipeline (col-002) gains entry-level analysis as a new dimension.

---

## RQ-3e: crt-004 (Co-Access Boosting)

### Current Implementation

Co-access tracking (`coaccess.rs` + `schema.rs`):

- **`CO_ACCESS` table**: `(min_entry_id, max_entry_id) -> CoAccessRecord { count: u32, last_updated: u64 }`. Keys are ordered to deduplicate symmetric pairs.
- **Pair generation**: `generate_pairs()` creates ordered pairs from entry IDs (max 10 entries = 45 pairs). Called in Step 5 of `record_usage_for_entries()`.
- **Session dedup**: `UsageDedup::filter_co_access_pairs()` ensures each pair recorded at most once per server session (agent-independent).
- **Boost computation**: `compute_search_boost()` and `compute_briefing_boost()` — for each anchor entry (top results), look up co-access partners, compute log-transformed boost capped at `MAX_CO_ACCESS_BOOST = 0.03` (search) or `MAX_BRIEFING_CO_ACCESS_BOOST = 0.01` (briefing).
- **Confidence integration**: `co_access_affinity()` computes `W_COAC * partner_score * avg_partner_confidence`, contributing up to 0.08 to effective confidence at query time.
- **Staleness**: Partners older than `CO_ACCESS_STALENESS_SECONDS` (30 days) are excluded.

### Impact from Cortical Implant

Hook injections create a fundamentally different co-access pattern:

1. **Injection co-access**: When the implant injects entries A, B, C into a prompt (col-007), these entries are "co-accessed" — they appear in the same context window. Should this feed CO_ACCESS?
   - **Yes**: Entries injected together share topical relevance. Recording their co-access strengthens future injection quality.
   - **Concern**: Injection happens on every prompt. If each prompt generates co-access pairs, the CO_ACCESS table inflates rapidly. A session with 50 prompts and 3 injections per prompt = 50 * 3 = 150 pair recordings (with dedup: 3 pairs, each updated 50 times).

2. **Volume management**: Current MCP co-access recording: ~5-10 retrievals per session, ~10-45 pairs. Implant co-access: potentially hundreds of pair updates per session (high count values). The log-transform in `co_access_boost()` naturally saturates at count=20, so high counts don't produce disproportionate boosts.

3. **Signal quality difference**: MCP co-access means "agent chose to retrieve these together." Injection co-access means "system chose to inject these together." The system's injection choices are based on semantic similarity to the prompt — entries injected together are likely related. This is a valid co-access signal, potentially higher quality than manual retrieval co-access.

### Recommended Approach: Feed Same CO_ACCESS Table with Session-Scoped Dedup

- **Same table**: Injection co-access feeds the same `CO_ACCESS` table. The co-access signal is valid and useful for improving future injection quality.
- **Session-scoped dedup**: Each (entry_a, entry_b) pair is recorded at most once per session, regardless of how many prompts co-injected them. This prevents count inflation.
- **No source discrimination**: The `CoAccessRecord` does not need a source field. The co-access relationship is symmetric and source-independent — what matters is that entries are used together, not how they came to be used together.
- **Pair generation**: The implant generates pairs from each injection set (typically 3-5 entries = 3-10 pairs). Uses existing `generate_pairs()` with `MAX_CO_ACCESS_ENTRIES = 10`.

### Migration Path

1. **col-007 ships**: Implant calls a new function (or extends `record_usage_for_entries()` with source="hook") that generates co-access pairs from injected entry sets. `UsageDedup::filter_co_access_pairs()` provides session dedup.
2. **No schema changes**: `CO_ACCESS` table and `CoAccessRecord` remain unchanged.
3. **crt-006 (Adaptive Embedding)**: Co-access pairs from injections feed the same training reservoir as MCP co-access pairs. The MicroLoRA adaptation benefits from richer co-access signal.

### Recommendation

**Keep as-is** — extend the existing co-access recording path to accept injection pairs. The `CO_ACCESS` table, `CoAccessRecord`, pair generation, boost computation, and session dedup all work without modification. The implant calls the same recording infrastructure with the same dedup guarantees.

---

## RQ-3f: col-001 (Outcome Tracking)

### Current Implementation

- **`OUTCOME_INDEX` table** (13th table): `(feature_cycle: &str, entry_id: u64) -> ()`. Populated when `context_store` creates an entry with `category: "outcome"` and non-empty `feature_cycle`.
- **Structured tags** (`outcome_tags.rs`): Required `type` tag (feature/bugfix/incident/process) + optional `gate`, `phase`, `result`, `agent`, `wave` tags. Strict validation with recognized key set.
- **Agent-authored**: Outcomes are stored by agents via `context_store` MCP tool. The agent writes a content description + structured tags + feature_cycle.
- **Usage in context_status**: Outcome entries are counted in the status report. They provide a historical record of what happened during a feature cycle.

### Impact from Cortical Implant

col-010 (Session Lifecycle) could auto-generate outcome entries from session end signals:

1. **Auto-generated outcomes**: When SessionEnd fires, the implant knows: session duration, feature context, agent role, whether tasks completed or failed (from TaskCompleted hook), number of tool calls, files modified. It could auto-store an outcome entry summarizing the session.

2. **Complementary, not replacement**: Agent-authored outcomes contain judgment and context that session metadata cannot provide. An agent writes "Gate 3a passed with 2 findings — scope narrowed per risk assessment." A session-end auto-outcome writes "Session completed in 45 minutes, 234 tool calls, feature col-002." These serve different purposes.

3. **Tag compatibility**: Auto-generated outcomes can use existing structured tags (`type:feature`, `phase:implementation`, `result:pass`). The `outcome_tags.rs` validation rules are compatible.

4. **Volume**: Auto-generated outcomes add 1-5 entries per session (one per feature phase touched). Agent-authored outcomes are 1-3 per feature cycle. The auto-generated ones are lower signal but higher frequency.

### Recommended Approach: Complement Agent Outcomes with Session Summaries

- **New tag value**: Add `type:session` to `VALID_TYPES` to distinguish auto-generated session outcomes from agent-authored feature outcomes.
- **New tag key**: Add `source:hook` or `source:auto` to indicate the outcome was generated by the implant, not an agent.
- **OUTCOME_INDEX**: Unchanged — auto-outcomes have a `feature_cycle` and are indexed normally.
- **Agent outcomes remain primary**: For retrospective analysis (col-002), agent-authored outcomes with `result:pass/fail/rework` carry more weight than session metadata.

### Migration Path

1. **col-006 ships**: No changes to col-001.
2. **col-010 ships**: SessionEnd hook auto-stores session outcome entries. Requires adding `"session"` to `VALID_TYPES` in `outcome_tags.rs` (1-line change + tests).
3. **No schema changes**: `OUTCOME_INDEX` table and `EntryRecord` accommodate auto-outcomes without modification.

### Recommendation

**Extend minimally** — Add `"session"` to `VALID_TYPES` and optionally add `"source"` to `RECOGNIZED_KEYS` in `outcome_tags.rs`. This is a 2-line validation change. The table schema, indexing, and query paths require no modification. Auto-generated outcomes complement agent outcomes for richer retrospective analysis.

---

## RQ-3g: vnc-003 (context_briefing)

### Current Implementation

`context_briefing` (`tools.rs:1631-1853`) produces role+task-scoped knowledge bundles:

1. **Identity + capability check**: Read capability required.
2. **Conventions lookup**: `query(topic=role, category="convention", status=Active)`.
3. **Duties lookup**: `query(topic=role, category="duties", status=Active)`.
4. **Semantic search**: Embeds task description, searches HNSW index (k=3, ef=32). Excludes quarantined entries. Feature boost: entries tagged with feature param sorted higher. crt-006 embedding adaptation applied.
5. **Co-access boost**: Briefing results get co-access boost (max 0.01, smaller than search's 0.03).
6. **Token budget**: Priority order conventions > duties > relevant_context. Character budget = `max_tokens * 4`. Default max_tokens=3000, range 500-10000.
7. **Usage recording**: All returned entry IDs recorded via `record_usage_for_entries()` (fire-and-forget).
8. **Response**: `Briefing { role, task, conventions, duties, relevant_context, search_available }`.

Key properties: Returns an **unordered bag** of knowledge entries. No session awareness. No injection history. No "what was already injected" tracking.

### Impact from Cortical Implant

The PreCompact hook (col-008) needs compaction defense — re-injecting critical context into the compacted window. `context_briefing` is the closest analog but has critical gaps:

1. **No session awareness**: Briefing doesn't know what entries were already injected during this session. It might re-inject the same entries that were just compacted (good) or miss entries that were injected but not in the semantic search results (bad).

2. **No injection history**: The implant needs to know "which entries did I inject earlier in this session?" to reconstruct context. Briefing has no concept of this.

3. **No prioritization for compaction defense**: Briefing prioritizes conventions > duties > task-relevant context. Compaction defense may need different prioritization: active decisions > current feature context > recent injections > conventions.

4. **Token budget mismatch**: Briefing targets <2000 tokens (char_budget = max_tokens * 4). Compaction defense may need a different budget — smaller for frequent compaction, larger for first compaction.

5. **Latency requirements**: PreCompact is the most latency-critical hook — it must return content synchronously before Claude Code proceeds. Briefing does HNSW search + embedding + co-access boost, which may exceed the <50ms target.

### Design Options for Compaction Defense

**Option A: Call briefing internally from PreCompact hook**
The implant calls `context_briefing` (via IPC to MCP server or direct library call). Simplest, but doesn't address session awareness or prioritization gaps. Latency concern: briefing does embedding + HNSW search.

**Option B: New `session_briefing()` query interface**
A new internal function (not an MCP tool) that takes: session context (role, task, feature), injection history (entry IDs injected this session), and compaction number (1st, 2nd, etc.). Returns a prioritized, token-budgeted payload optimized for compaction defense. Can reuse conventions/duties lookup from briefing but adds injection history re-ranking.

**Option C: Pre-computed compaction payload**
The implant maintains a "compaction payload" in memory, updated after every injection. When PreCompact fires, the payload is ready — no database query needed. This meets the <50ms target trivially but requires the implant to maintain session state.

### Recommended Approach: Option B + C Hybrid

- **Option C for fast path**: The implant maintains a pre-computed compaction payload (entry IDs + summaries) updated on every injection and session state change. This is the default path — instant, no I/O.
- **Option B for cold start**: If the implant restarts mid-session (daemon died, process killed), it falls back to a `session_briefing()` query that reconstructs the payload from injection history stored in Unimatrix.
- **Briefing stays as-is**: `context_briefing` MCP tool remains unchanged for agent-initiated briefing requests. It is NOT the compaction defense path.

### Evolution Needed

| Aspect | Current Briefing | Compaction Defense |
|--------|-----------------|-------------------|
| Trigger | Agent calls MCP tool | PreCompact hook fires |
| Session awareness | None | Must know injection history |
| Prioritization | conventions > duties > context | active decisions > feature context > recent injections |
| Token budget | Default 3000 tokens | Adaptive: smaller on frequent compaction |
| Latency | ~200ms (embed + HNSW) | <50ms (pre-computed) |
| State | Stateless | Session-scoped |

### Migration Path

1. **col-006 ships**: No changes to vnc-003.
2. **col-007 ships (injection)**: Implant tracks injection history per session in memory.
3. **col-008 ships (compaction)**: Implements pre-computed compaction payload. New `session_briefing()` function in `unimatrix-server` (or `unimatrix-core`) as fallback. `context_briefing` MCP tool unchanged.
4. **Later**: If compaction defense needs server-side state (for daemon restart recovery), a new `SESSION_STATE` table may be added. This is a col-008 design decision, not a vnc-003 change.

### Recommendation

**Keep as-is for MCP tool; build parallel path for compaction defense.** `context_briefing` serves its purpose well for agent-initiated orientation. Compaction defense requires a fundamentally different interface: session-aware, pre-computed, latency-optimized. These are separate concerns that share underlying data (conventions, duties, entries) but differ in query pattern, prioritization, and state model. Do not overload `context_briefing` with session state — build col-008's compaction payload as a new capability.

---

## Blocking vs. Incremental Matrix

| Change | Feature | Blocks col-006? | Blocks col-007? | Blocks col-008? | Blocks col-009? | Can Ship After? |
|--------|---------|:---------------:|:---------------:|:---------------:|:---------------:|:---------------:|
| Structured event ingestion in unimatrix-observe | col-002 | No | No | No | No | Yes (col-010) |
| Source discriminator on usage recording | crt-001 | No | No | No | **Yes** | col-009 |
| Session-scoped injection dedup in UsageDedup | crt-001 | No | No | No | **Yes** | col-009 |
| Batch session-end usage recording | crt-001 | No | No | No | **Yes** | col-009 |
| No changes to confidence formula | crt-002 | No | No | No | No | N/A |
| Injection co-access recording | crt-004 | No | **Yes** | No | No | col-007 |
| `"session"` type in outcome tags | col-001 | No | No | No | No | col-010 |
| Pre-computed compaction payload | vnc-003 | No | No | **Yes** | No | col-008 |
| `session_briefing()` fallback function | vnc-003 | No | No | **Yes** | No | col-008 |

**Key finding: No existing feature changes block col-006.** The hook transport layer can ship with zero modifications to existing features. Changes cascade from the features that USE the transport (col-007 through col-010), not from the transport itself.

---

## Simplification Ledger

| Component | File(s) | Current LOC | Post-Implant Status | Estimated Savings |
|-----------|---------|-------------|---------------------|-------------------|
| JSONL parser | `parser.rs` | ~195 | Legacy/fallback | ~150 LOC becomes dead path |
| Feature attribution | `attribution.rs` | ~137 | 90% becomes fallback | ~120 LOC becomes dead path |
| Session file management | `files.rs` | ~124 | Cleanup-only | ~80 LOC becomes dead path |
| observe.sh hook script | `hooks/observe.sh` | ~47 lines | Eliminated | 47 lines removed |
| Cold restart heuristic | `metrics.rs` (subset) | ~15 | Unnecessary with explicit sessions | 15 LOC removable |
| Phase extraction from TaskCreate | `metrics.rs` (subset) | ~25 | Simplified with explicit phases | 15 LOC simplifiable |
| **Total estimated** | | | | **~400 LOC dead/removable** |

No tables are removed. No tables are added by the impact assessment (new tables are col-006+ design decisions). All existing tables remain useful:

| Table | Status | Notes |
|-------|--------|-------|
| ENTRIES + 5 indexes | Unchanged | |
| VECTOR_MAP | Unchanged | |
| COUNTERS | Unchanged | |
| AGENT_REGISTRY | Unchanged | |
| AUDIT_LOG | Unchanged | |
| FEATURE_ENTRIES | Unchanged | Hook injections populate via session feature context |
| CO_ACCESS | Unchanged | Injection pairs feed same table |
| OUTCOME_INDEX | Unchanged | Session outcomes indexed normally |
| OBSERVATION_METRICS | Unchanged | MetricVectors source-agnostic |

---

## Risk Register

| ID | Risk | Severity | Affected Feature | Mitigation |
|----|------|----------|-----------------|------------|
| R-01 | Hook injection inflates access_count, distorting usage_score in confidence formula | High | crt-001, crt-002 | Session-scoped dedup: max 1 access count per entry per session. Log-transform in usage_score caps at ~50 accesses. |
| R-02 | Bulk implicit helpful votes dilute Wilson score signal quality | Medium | crt-002 | Wilson minimum 5-vote guard ensures at least 5 sessions of injection before deviation from neutral prior. Entries with mixed explicit+implicit votes will have higher total counts, naturally improving Wilson bound precision. |
| R-03 | Co-access pairs from injections overwhelm pairs from MCP retrievals | Low | crt-004 | Session dedup limits injection pairs to once per session. Log-transform boost saturates at count=20. Staleness cleanup at 30 days. |
| R-04 | JSONL path and structured event path produce different MetricVectors for same session | Medium | col-002, col-002b | During migration: validate that structured events produce identical ObservationRecords as JSONL parsing. Add cross-path equivalence tests. |
| R-05 | Auto-generated session outcomes crowd out agent-authored outcomes in searches | Low | col-001 | Distinguish via `type:session` tag. Agent-authored outcomes (`type:feature/bugfix`) have richer content and will rank higher in semantic search. |
| R-06 | Compaction defense latency exceeds budget when falling back to session_briefing() | High | vnc-003 | Pre-computed payload is the primary path. session_briefing() is fallback only. If fallback latency is too high, degrade gracefully (inject whatever is pre-computed, skip server query). |
| R-07 | UsageDedup memory grows unbounded as injection tracking adds entries | Low | crt-001 | UsageDedup is cleared on server restart. Within a session, injection tracking adds O(entries * sessions) pairs, which is bounded by knowledge base size. No growth concern at current scale. |
| R-08 | Existing OBSERVATION_METRICS entries become incomparable when new MetricVector fields are added | Low | col-002b | New fields use `#[serde(default)]`, so old MetricVectors deserialize with default values. Baseline computation handles zero-variance (NoVariance status) and zero-mean (NewSignal status) gracefully. |
| R-09 | Session-end outcome signaling fails silently if session ends abnormally (crash, network drop) | Medium | col-001, crt-001, crt-002 | Implant should persist injection history to a lightweight sidecar file. On next startup, detect orphaned sessions and either discard or process accumulated data. |
| R-10 | Two recording paths (MCP + hook) create race conditions on EntryRecord field updates | Low | crt-001, crt-002 | redb single-writer model serializes all writes. spawn_blocking ensures sequential write access. No concurrent write contention possible within a single process. If implant writes via IPC to server, the server serializes. If implant writes directly to redb, the flock mechanism (vnc-004) prevents dual writers. |

---

## Cross-Feature Interaction Summary

```
col-006 (Hook Transport)
   │
   ├──► col-007 (Context Injection)
   │     ├── Feeds crt-004 co-access pairs (injection co-access)
   │     ├── Feeds crt-001 access tracking (session-scoped)
   │     └── Populates FEATURE_ENTRIES (session feature context)
   │
   ├──► col-008 (Compaction Resilience)
   │     ├── Uses pre-computed payload (NOT context_briefing)
   │     ├── Falls back to session_briefing() on cold start
   │     └── Tracks compaction frequency for col-002 observation
   │
   ├──► col-009 (Closed-Loop Confidence)
   │     ├── Feeds crt-001 batch session-end usage recording
   │     ├── Feeds crt-002 helpfulness signals (implicit, session outcome)
   │     └── Subject to Wilson score minimum sample guard (5 votes)
   │
   ├──► col-010 (Session Lifecycle)
   │     ├── Feeds col-001 auto-generated session outcomes
   │     ├── Provides explicit session boundaries for col-002
   │     ├── Replaces JSONL-inferred sessions in unimatrix-observe
   │     └── Enables session-scoped dedup in crt-001/crt-004
   │
   └──► col-011 (Semantic Agent Routing)
         └── Queries col-001 outcomes + conventions (read-only, no impact)
```

---

## Appendix: Key Code Locations

| Feature | Primary Files | LOC (approx) |
|---------|---------------|:------------:|
| col-002 | `crates/unimatrix-observe/src/{lib,parser,types,metrics,attribution,report,files,detection/}.rs` | ~2,800 |
| col-002b | `crates/unimatrix-observe/src/baseline.rs` + detection extensions | ~500 |
| crt-001 | `crates/unimatrix-server/src/{usage_dedup,server}.rs` (record_usage_for_entries) | ~350 |
| crt-002 | `crates/unimatrix-server/src/confidence.rs` | ~200 |
| crt-004 | `crates/unimatrix-server/src/coaccess.rs` + `crates/unimatrix-store/src/schema.rs` (CO_ACCESS) | ~270 |
| col-001 | `crates/unimatrix-server/src/outcome_tags.rs` + `crates/unimatrix-store/src/schema.rs` (OUTCOME_INDEX) | ~160 |
| vnc-003 briefing | `crates/unimatrix-server/src/tools.rs:1631-1853` | ~220 |
