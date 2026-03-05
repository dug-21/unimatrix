# ASS-015: Existing Signals & Observation Infrastructure

## Executive Summary

Unimatrix already possesses a multi-layered signal and observation infrastructure spanning 4 research spikes (ASS-011, ASS-013, ASS-014), 3 major feature families (crt-001–005, col-001, col-002), and 6 crates (store, vector, embed, core, server, observe). The system captures signals from 7 distinct sources:

1. **Explicit user feedback** — helpful/unhelpful votes on MCP tool calls
2. **Implicit outcome signals** — session success/rework inferred from agent behavior
3. **Hook-driven telemetry** — tool calls, context injections, compaction events
4. **Observation telemetry** — 21 detection rules analyzing session activity
5. **Usage tracking** — access counts, co-access pairs
6. **Confidence evolution** — 6-component weighted composite from all signals
7. **Coherence metrics** — freshness, graph structure, contradiction detection

Current gaps are **not in signal capture** but in **passive knowledge acquisition** — moving from signals about *entry usage* to signals about *agent learning and knowledge creation*.

---

## 1. ASS-013 Tool Call Observation (32 Signal Types)

ASS-013 analyzed 1,180 tool calls across 4 sessions (crt-006 feature cycle), identifying 32 distinct signal types across 6 categories.

### Key Signals Already Detected

| Signal | Detection | Measurable | Current Use |
|--------|-----------|-----------|-------------|
| Permission Retries | Pre/Post differential | YES | Hotspot rule (col-002) |
| Search Miss Rate | Empty Grep/Glob responses | YES (32% baseline) | Hotspot rule (col-002) |
| Parallel Call Rate | Same-timestamp tool calls | YES (25%) | Metric vector |
| Context Load | KB read before first write | YES (~123KB cold restart) | Hotspot rule (col-002) |
| Edit Response Bloat | Edit tool response > 50KB | YES (44% of context) | Hotspot rule (col-002) |
| Cold Restart | Gap > 30min in timestamp sequence | YES | Hotspot rule (col-002) |

### Critical Finding

Knowledge interaction drops to **0% during delivery** — agents don't consult Unimatrix during implementation. This is a major gap for passive knowledge acquisition.

### Telemetry Gaps

| Gap | Impact |
|-----|--------|
| Nested subagent types invisible | 26/31 SubagentStop have empty agent_type |
| No agent-to-task correlation | Can't link agent output to specific task |
| No context window size signal | Can't measure actual context pressure |
| Agent role not on tool calls | All subagent calls share parent session_id |

---

## 2. COL-002 Retrospective Pipeline (21 Detection Rules)

### Agent Hotspots (7 Rules)

| Rule | Threshold | Baseline | Data Source |
|------|-----------|----------|-------------|
| context_load | >100KB reads before first write | 123KB | PostToolUse response_size |
| lifespan | Agent duration >45min | 40min | SubagentStart/Stop |
| file_breadth | Distinct files read >threshold | ~20 files | Read tool inputs |
| reread_rate | Same file read 3+ times | High post-delivery | Read tool inputs |
| mutation_spread | Write/Edit to >N distinct files | 9 files | Write/Edit inputs |
| compile_cycles | `cargo test` invocations >N | 8 in 15min | Bash pattern match |
| edit_bloat | Edit response >50KB per file | 91KB avg | PostToolUse response_size |

### Friction Hotspots (4 Rules)

| Rule | Signal | Detection |
|------|--------|-----------|
| permission_retries | Pre/Post differential per tool | count(Pre) - count(Post) |
| sleep_workarounds | Any `sleep` in Bash | Regex pattern match |
| search_via_bash | grep/find in Bash instead of Grep/Glob | Ratio > 5% |
| output_parsing_struggle | Repeated Bash with different output filters | Multiple cargo test with same query |

### Session Hotspots (5 Rules)

session_timeout, cold_restart, coordinator_respawns, post_completion_work, rework_events

### Scope Hotspots (5 Rules)

source_file_count, design_artifact_count, adr_count, post_delivery_issues, phase_duration_outlier

### MetricVector Structure

```rust
pub struct MetricVector {
    pub computed_at: u64,
    pub universal: UniversalMetrics,  // 10+ metrics
    pub phases: BTreeMap<String, PhaseMetrics>,
}
```

Storage: `observation_metrics` table in SQLite: `feature_cycle -> bincode(MetricVector)`

---

## 3. ASS-011 Hook-Driven Orchestration

### Validated Hook Mechanisms

| Mechanism | Event | Direction | Latency |
|-----------|-------|-----------|---------|
| SubagentStart additionalContext | SubagentStart | Inject prompt context | 72ms |
| PreToolUse updatedInput | PreToolUse | Modify tool parameters | 56-92ms |
| PostToolUse updatedMCPToolOutput | PostToolUse | Replace/enrich MCP output | 80ms |
| PostToolUse additionalContext | PostToolUse | Inject system-reminder | 73-94ms |

**Signal Extraction Opportunity:** `transcript_path` provides access to full conversation — hooks could use this for context-aware decisions.

---

## 4. ASS-014 Cortical Implant (Schema v4-v5 Design)

### 3 New Tables Proposed

| Table | Purpose |
|-------|---------|
| SESSIONS | Session lifecycle tracking (14 fields) |
| INJECTION_LOG | Per-injection event records (30-day GC) |
| SIGNAL_QUEUE | Pending confidence signals (transient) |

### Signal Lifecycle

```
PostToolUse event → INJECTION_LOG (ephemeral)
  → [session ends with success] → SIGNAL_QUEUE: Helpful → confidence pipeline
  → [session ends with rework]  → SIGNAL_QUEUE: Flagged → retrospective (human review)
```

**Critical design:** Auto-apply Helpful only. Flagged signals go to retrospective for human review — NOT auto-applied as unhelpful.

---

## 5. Confidence System (crt-001 through crt-005)

### 6-Component Weighted Composite

| Component | Weight | Signal |
|-----------|--------|--------|
| Base Quality | 0.18 | Status (Active/Proposed/Deprecated/Quarantined) |
| Usage Frequency | 0.14 | Log(1 + access_count), capped at 50 |
| Freshness | 0.18 | Exponential decay (1-week half-life) |
| Helpfulness | 0.14 | Wilson score from (helpful_count, unhelpful_count) |
| Correction Chain | 0.14 | f(correction_count) |
| Creator Trust | 0.14 | human=1.0, system=0.7, agent=0.5, other=0.3 |
| Co-Access Affinity | 0.08 | Query-time: ln(1+partners)/ln(51) * avg_partner_conf |

### Fire-and-Forget Usage Recording

```rust
// In tools.rs — context_search/lookup/briefing/get
self.record_usage_for_entries(
    &identity.agent_id, identity.trust_level,
    &target_ids, params.helpful, params.feature.as_deref(),
).await;  // Async, does NOT block MCP response
```

---

## 6. MCP Tool Call Signal Points (10 Tools)

| Tool | Signal Extracted | Stored Where |
|------|------------------|--------------|
| context_search | query text, helpful, feature | AUDIT_LOG, entry.last_accessed_at |
| context_lookup | filter params, helpful, feature | AUDIT_LOG, entry.last_accessed_at |
| context_get | entry ID, helpful, feature | AUDIT_LOG, entry.last_accessed_at |
| context_store | new content, feature_cycle, category | ENTRIES, FEATURE_ENTRIES |
| context_correct | original_id, new content, reason | ENTRIES, correction_count++ |
| context_deprecate | entry ID, reason | ENTRIES, status change |
| context_briefing | role, task, helpful, feature | AUDIT_LOG, entry.last_accessed_at |
| context_status | maintenance reads | AUDIT_LOG |
| context_quarantine | entry ID, action | ENTRIES, status change |
| context_enroll | agent, trust level | AGENT_REGISTRY |

Every MCP call writes to AUDIT_LOG with: agent_id, tool_name, entry_ids, outcome, feature, parameters_hash.

---

## 7. Gaps for Passive Knowledge Acquisition

### What's Missing

| Gap | Current State | Opportunity |
|-----|---------------|-------------|
| Agent Learning Trajectory | No per-agent tracking | Track helpfulness by creator/injector |
| Knowledge Degradation | No detection | Confidence drift + access cliff alerts |
| Query Drift | No analysis | Trending on search query space over time |
| Entry Obsoletion | Manual deprecate only | Access frequency cliff detection |
| Cross-Agent Knowledge Transfer | Not measured | Entry creation correlated with agent type |
| Compaction Losses | Not tracked | PreCompact hook captures what survives |
| Rework Root Cause | Detected via hotspots | Causal link to specific entries |
| Procedural Knowledge | Not extracted | AUDIT_LOG agent sequencing analysis |
| File Dependency Graphs | Not learned | Read-before-edit chain analysis |

### What CRT-001–005 Does NOT Do

- Does NOT detect entry effectiveness *within* a feature
- Does NOT measure if agent understanding improved
- Does NOT detect when confidence *should* drop (data-driven)
- Does NOT learn new signals from observed behavior

### What COL-002 Does NOT Do

- Does NOT link hotspots to specific knowledge entries
- Does NOT detect *new* patterns (only known rules)
- Does NOT generate automated recommendations
- Does NOT track correlation between knowledge and hotspots

---

## 8. Signal Source Summary

| Source | Captured In | Current Use | ASS-015 Gap |
|--------|-------------|------------|-------------|
| Explicit votes | ENTRIES.helpful_count | Confidence | Drift detection |
| Session outcomes | SIGNAL_QUEUE | Confidence | Link to injected entries |
| Tool calls | AUDIT_LOG | Hotspot analysis | Query drift, procedure extraction |
| Access patterns | ENTRIES | Confidence/freshness | Usage trajectory, cliff detection |
| Co-access | CO_ACCESS | Search boost | Dependency graph learning |
| Phase timing | MetricVector | Baseline comparison | Anomaly alerts |
| Agent behavior | Hotspot rules | Anomaly detection | Norm violations as signals |
| File access | AUDIT_LOG | Context load hotspot | Read-before-edit chains |
| Corrections | ENTRIES | Confidence component | Cascade effect tracking |
| Status transitions | ENTRIES.status | Manual decisions | Root cause trending |

Of the 70+ signals identified across the codebase, **26 (37%) are already tracked**. The remaining 44 require new extraction logic but operate on data already present in existing JSONL records and SQLite tables.
