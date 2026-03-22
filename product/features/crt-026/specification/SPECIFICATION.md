# SPECIFICATION: crt-026 — WA-2 Session Context Enrichment

GH Issue: #341

---

## Objective

`context_search` is currently session-blind: every query is ranked by the same six-term fused
score regardless of what the current session has been producing. crt-026 adds a category
histogram to `SessionState`, threads it into the search ranking pipeline as a first-class
`FusedScoreInputs`/`FusionWeights` dimension, and surfaces the histogram summary in the UDS
`CompactPayload` synthesis output. The result is an implicit, zero-configuration session context
signal that surfaces more relevant results without any ML or agent-declared phase intent.

---

## Domain Model

### Ubiquitous Language

| Term | Definition |
|---|---|
| **Session** | A bounded agent interaction identified by `session_id`. Tracked in `SessionRegistry` across MCP and UDS calls. |
| **CategoryHistogram** | A per-session in-memory map (`HashMap<String, u32>`) accumulating the count of each knowledge category stored during the session. Not persisted; reset on session registration. |
| **Histogram Affinity Boost** | An additive score term applied per search result: `p(entry.category) * w_phase_histogram`, where `p(category)` is the category's fraction of total session stores. Represents the implicit signal (what the session has been doing). |
| **Explicit Phase Term** | A separate boost term using `current_phase` (from WA-1/crt-025). **Reserved at `w_phase_explicit=0.0` in crt-026; deferred to W3-1.** |
| **Cold Start** | State where the session histogram is empty (no successful stores yet). Boost is exactly `0.0` for all entries — produces identical ranking to the pre-crt-026 pipeline. |
| **FusedScoreInputs** | The per-candidate feature vector passed to `compute_fused_score`. Extended in crt-026 with `phase_histogram_norm: f64`. |
| **FusionWeights** | The weight vector read from `InferenceConfig` and passed to `compute_fused_score`. Extended in crt-026 with `w_phase_histogram: f64` and `w_phase_explicit: f64`. |
| **ServiceSearchParams** | The internal service-layer search parameters struct. Extended in crt-026 with `session_id: Option<String>` and `category_histogram: Option<HashMap<String, u32>>`. |
| **PreCompact** | The UDS hook event that fires before context compaction. The category histogram summary is injected into `format_compaction_payload` output at this point. |
| **W3-1 initialization value** | `w_phase_histogram=0.02` is the ASS-028 calibrated seed weight for this dimension in the W3-1 GNN cold-start. Carries the full session signal budget while `w_phase_explicit` is deferred; W3-1 refines and rebalances from there. |

### Entities and Relationships

```
SessionRegistry
  └── SessionState (per session_id)
        ├── current_phase: Option<String>          ← WA-1, read-only in crt-026
        └── category_counts: HashMap<String, u32>  ← NEW in crt-026

context_store handler
  └── on success (non-duplicate):
        └── record_category_store(session_id, category) → mutates SessionState.category_counts

context_search handler (MCP path)
  └── get_category_histogram(session_id)     ← pre-resolves before ServiceSearchParams
      get_state(session_id).current_phase    ← pre-resolves alongside histogram
      └── ServiceSearchParams
            ├── session_id: Option<String>
            └── category_histogram: Option<HashMap<String, u32>>  ← NEW

handle_context_search (UDS path)
  └── session_id from HookRequest::ContextSearch payload field
      └── same pre-resolution into ServiceSearchParams as MCP path

SearchService::search()
  └── per-candidate FusedScoreInputs
        └── phase_histogram_norm = p(entry.category) from category_histogram
      └── compute_fused_score(inputs, weights)
            └── + w_phase_histogram * phase_histogram_norm
                + w_phase_explicit * 0.0  (always zero, reserved)

handle_compact_payload (UDS PreCompact)
  └── get_category_histogram(session_id)
      └── format_compaction_payload appends histogram summary block (if non-empty)
```

---

## Functional Requirements

**FR-01: `SessionState.category_counts` field**
`SessionState` must have a `category_counts: HashMap<String, u32>` field. It is initialized to
`HashMap::new()` in `register_session`. The field holds a mutable count per knowledge category
string, scoped to the lifetime of the session in the registry.

**FR-02: `record_category_store()` method on `SessionRegistry`**
`SessionRegistry` must expose a `pub fn record_category_store(&self, session_id: &str, category: &str)`
method. When the session exists, it increments `category_counts[category]` by 1 under the
existing sessions `Mutex`. When `session_id` does not match a registered session, the method is
a silent no-op — consistent with `record_injection`. Lock hold is synchronous and bounded to
microseconds; no `spawn_blocking` is needed.

**FR-03: `get_category_histogram()` getter on `SessionRegistry`**
`SessionRegistry` must expose a `pub fn get_category_histogram(&self, session_id: &str) -> HashMap<String, u32>`
method. It returns a clone of the session's `category_counts`, or an empty `HashMap` if the session
is not registered. This method is the sole read path for the histogram — called from the tool handler
before constructing `ServiceSearchParams`, and from `handle_compact_payload`.

**FR-04: `context_store` handler calls `record_category_store` after non-duplicate success**
After the duplicate guard (`insert_result.duplicate_of.is_some()` check, current step 7), before
confidence seeding (step 8), the `context_store` handler must call:
```
if let Some(ref sid) = ctx.audit_ctx.session_id {
    self.session_registry.record_category_store(sid, &params.category);
}
```
A store that is a duplicate (same entry already exists) must NOT increment the histogram.
A store where `session_id` is `None` must NOT attempt to call `record_category_store`.

**FR-05: `ServiceSearchParams` carries pre-resolved session context**
`ServiceSearchParams` must gain two new fields:
- `session_id: Option<String>` — the session identifier, for logging and tracing
- `category_histogram: Option<HashMap<String, u32>>` — pre-resolved histogram clone, or `None` when no session exists or histogram is empty

These are data-carrier fields. No logic belongs in `ServiceSearchParams` itself.

**FR-06: `context_search` handler pre-resolves histogram and threads it through**
In the `context_search` MCP tool handler, before constructing `ServiceSearchParams`, the handler must:
1. Read `ctx.audit_ctx.session_id`
2. If `Some(sid)`, call `self.session_registry.get_category_histogram(sid)` — store as `Option<HashMap>` (use `None` if returned map is empty)
3. Populate `ServiceSearchParams.session_id` with `ctx.audit_ctx.session_id.clone()`
4. Populate `ServiceSearchParams.category_histogram` with the resolved histogram

This follows the crt-025 SR-07 snapshot pattern: session state is read once before any `await`, avoiding races with concurrent session mutations.

**FR-07: UDS `handle_context_search` pre-resolves histogram identically**
`handle_context_search` in `uds/listener.rs` must also pre-resolve the histogram from
`session_registry` using the `session_id` field from `HookRequest::ContextSearch`. The
`session_id` in the UDS path originates from the hook payload field — it is NOT derived from
an `audit_ctx` equivalent. The pre-resolution produces the same `ServiceSearchParams` fields as
the MCP path. This ensures the histogram affinity boost fires on hook-driven searches as well as
direct MCP calls (OQ-04 resolved).

**FR-08: `FusedScoreInputs` gains `phase_histogram_norm` field**
`FusedScoreInputs` must gain a `phase_histogram_norm: f64` field representing the normalized
category probability `p(entry.category)` from the session histogram. The value is in `[0.0, 1.0]`.
When the histogram is absent or empty, this field is `0.0`. The WA-2 extension stub at line 55 of
`search.rs` is the integration contract — the stub comment must be removed/replaced with the field.

**FR-09: `FusionWeights` gains `w_phase_histogram` and `w_phase_explicit` fields**
`FusionWeights` must gain:
- `w_phase_histogram: f64` — weight for the histogram affinity term, default `0.02` (full session signal budget, ADR-004)
- `w_phase_explicit: f64` — weight for the explicit phase term, default `0.0` (reserved for W3-1; always zero in crt-026)

`FusionWeights::from_config()` must read both from `InferenceConfig`. The WA-2 extension stub at
line 89 of `search.rs` is the integration contract.

**FR-10: `compute_fused_score` integrates the histogram term**
The `compute_fused_score` pure function must include the histogram affinity term:
```
score += weights.w_phase_histogram * inputs.phase_histogram_norm
       + weights.w_phase_explicit  * inputs.phase_explicit_norm  // always 0.0 in crt-026
```
The WA-2 extension stub at line 179 of `search.rs` is the integration contract. The existing
sum-invariant documented on `FusionWeights` (`w_sim + w_nli + w_conf + w_coac + w_util + w_prov <= 1.0`)
must be updated in the doc-comment to include the phase terms. With defaults, the new sum is
`0.95 + 0.02 + 0.0 = 0.97`, within the `<= 1.0` invariant; no existing weight defaults need adjustment.

**FR-11: `InferenceConfig` gains `w_phase_explicit` and `w_phase_histogram` config fields**
`InferenceConfig` (in `infra/config.rs`) must gain:
- `#[serde(default = "default_w_phase_explicit")] pub w_phase_explicit: f64` — default `0.0`
- `#[serde(default = "default_w_phase_histogram")] pub w_phase_histogram: f64` — default `0.02`

Both follow the `default_w_*` pattern of the six existing fusion weight fields. `InferenceConfig::validate()`
must accept `sum = 0.97` cleanly (original six fields still sum to 0.95; phase fields excluded from
that check). Existing tests asserting the pre-WA-2 sum of `0.95` remain valid — `validate()` only
checks the original six-field sum.

**FR-12: Category histogram summary in `format_compaction_payload`**
`handle_compact_payload` must call `session_registry.get_category_histogram(session_id)` and pass
the result to `format_compaction_payload`. When the histogram is non-empty, `format_compaction_payload`
appends an informational block to the payload content:
```
Recent session activity: decision × 3, pattern × 2
```
Format rules:
- Emit only categories with count `> 0`
- Sort by count descending
- Cap at top-5 categories
- If histogram is empty, omit the block entirely (no blank line, no header)
- The block must fit within the existing `MAX_INJECTION_BYTES` budget (block is < 100 bytes for typical sessions)

---

## Non-Functional Requirements

**NFR-01: Lock hold latency**
`record_category_store` and `get_category_histogram` hold the `sessions` Mutex for microseconds
(a `HashMap` counter increment or a clone). No I/O, no `spawn_blocking`, no `await` inside the
lock. This is the same contract as `record_injection` and all other `SessionRegistry` methods.

**NFR-02: Cold-start safety**
When the session histogram is empty — including the case where `session_id` is `None`, the session
is not registered, or no `context_store` calls have succeeded yet — the `phase_histogram_norm` for
all entries must be `0.0`, and the `compute_fused_score` output must be bit-for-bit identical to
the pre-crt-026 result. No behavioral regression for sessions without histogram data.

**NFR-03: Backward compatibility — no schema migration**
`category_counts` is in-memory, per-session state only. No database tables are added. No schema
version bump. Sessions that pre-date crt-026 (or reconnect after a server restart) start with an
empty histogram; cold-start safety (NFR-02) covers this case.

**NFR-04: Boost bounded**
The maximum histogram boost per entry with defaults is `w_phase_histogram * 1.0 = 0.02` (at p=1.0
concentration). This prevents a weak-similarity entry from overriding a high-NLI-score entry. When
W3-1 eventually enables `w_phase_explicit`, W3-1 will learn the appropriate split — the combined
max will depend on W3-1's learned values. The 0.05 headroom from WA-0 provides clearance.

**NFR-05: UDS hook timeout budget**
The PreCompact histogram summary is a string-formatting operation on pre-resolved in-memory data.
No I/O, no embedding, no SQL. The operation adds negligible latency and must not approach the
40ms `HOOK_TIMEOUT` budget.

**NFR-06: W3-1 dimension compatibility**
`FusedScoreInputs.phase_histogram_norm` and `FusionWeights.w_phase_histogram` must be named,
stable, learnable dimensions. W3-1 (GNN) initializes from `w_phase_histogram=0.02` (the
ASS-028 calibrated value) and refines from there. At 0.02, the boost is detectable in realistic
sessions; test fixtures using p=1.0 concentration observe a score delta of exactly 0.02.

**NFR-07: No new crates**
All changes are within `crates/unimatrix-server`. No new workspace members.

---

## Acceptance Criteria

All AC-IDs flow from SCOPE.md. AC-07 is explicitly dropped (see note below).

**AC-01** `SessionState` has a `category_counts: HashMap<String, u32>` field, initialized to
`HashMap::new()` in `register_session`.
*Verification*: unit test confirms field is empty after `register_session`.

**AC-02** A successful `context_store` call increments `category_counts[category]` by 1 in the
session registry. A duplicate store (`insert_result.duplicate_of.is_some()`) does NOT increment
the histogram.
*Verification*: unit test: store same entry twice; histogram shows count = 1 after first, still 1 after second.

**AC-03** `record_category_store` is a silent no-op when `session_id` is `None` or the session is
not registered. No panic, no error, no side effect.
*Verification*: unit test: call `record_category_store` with an unknown session_id; histogram map is unchanged.

**AC-04** `ServiceSearchParams` has a `session_id: Option<String>` field.
*Verification*: struct definition review; compilation.

**AC-05** The `context_search` handler passes `ctx.audit_ctx.session_id.clone()` into
`ServiceSearchParams.session_id` and passes the pre-resolved histogram into
`ServiceSearchParams.category_histogram`.
*Verification*: unit test on handler: search with a session that has prior stores; confirm
`ServiceSearchParams` is constructed with correct values (via test double or inspection).

**AC-06** When `session_id` is present and the session histogram is non-empty, `SearchService`
applies the histogram affinity boost (`p(category) * w_phase_histogram`) to each result entry's
fused score via `compute_fused_score`.
*Verification*: see AC-12.

**AC-07** — **DROPPED.**
The explicit phase affinity boost (`phase_category_weight * w_phase_explicit`) is deferred to
W3-1 (OQ-03 resolved). `w_phase_explicit` defaults to `0.0`; the `phase_explicit_norm` input
to `compute_fused_score` is always `0.0` in crt-026. AC-07 would test always-zero behavior;
testing it as written would require a `phase_category_weight` mapping table that intentionally
does not exist in this feature. AC-07 is excluded from this specification and must not appear
in the ACCEPTANCE-MAP.

**AC-08** When the session histogram is empty (cold start), the affinity boost is `0.0` for all
entries. `SearchService` output is identical to the pre-crt-026 pipeline.
*Verification*: unit test: search with no prior stores; assert final scores equal to scores produced
without `session_id` / with empty histogram.

**AC-09** Both `w_phase_explicit` (default `0.0`) and `w_phase_histogram` (default `0.02`) are
configurable in `[inference]` config section, using the `default_w_*` serde pattern.
*Verification*: config round-trip test; `InferenceConfig::default()` returns correct values.

**AC-10** The histogram term is applied INSIDE `compute_fused_score` as a first-class dimension,
not as a post-pipeline additive step. `final_score = compute_fused_score(&inputs, &weights) * status_penalty`
— the application order is unchanged; the boost participates in the fused score before `status_penalty`.
*Verification*: code review of `compute_fused_score`; confirmed by AC-12 score delta calculation.

**AC-11** The `CompactPayload` synthesis output includes a `Recent session activity: ...` block
when the session has a non-empty histogram. The block is omitted when the histogram is empty.
*Verification*: unit test on `format_compaction_payload`: call with non-empty histogram → assert
block present; call with empty histogram → assert block absent.

**AC-12** A result whose category appears in the session histogram at `p=1.0` (e.g., all stores
are category `decision`) ranks higher than an otherwise equal result (same similarity, NLI score,
confidence, co-access, util, prov) whose category is absent from the histogram. The score delta
must be ≥ `w_phase_histogram * 1.0 = 0.02` (the maximum histogram boost at default weight).
*Verification*: unit test with two synthetic `ScoredEntry` values equal on all dimensions except category;
assert `score(decision) - score(other) ≥ 0.02`.

**AC-13** A result whose category is NOT in the session histogram receives `phase_histogram_norm = 0.0`
and boost `= 0.0` from the histogram term.
*Verification*: unit test: populate histogram with category `decision`; search result has category
`lesson-learned`; assert `phase_histogram_norm = 0.0`.

**AC-14** The WA-2 extension stubs in `search.rs` (`FusedScoreInputs` line 55, `FusionWeights` line 89,
`compute_fused_score` line 179) are resolved. Stub comments are replaced with the implemented field
declarations and doc-comments.
*Verification*: code review; no `WA-2 extension:` stub comment remains in `search.rs`.

---

## User Workflows

### Workflow 1: Agent in a Design Session

1. SM calls `context_cycle(type: "phase", phase: "design", topic: "crt-026")` — sets `current_phase` (WA-1, not modified here).
2. Specialist agents call `context_store(category: "decision", ...)` × 3, `context_store(category: "pattern", ...)` × 2.
3. After each successful non-duplicate store, `record_category_store` increments the histogram.
4. Session histogram is now: `{ "decision": 3, "pattern": 2 }`, total = 5.
5. Agent calls `context_search(query: "how to handle session registry access", session_id: "sid-abc")`.
6. Handler pre-resolves histogram: `{ "decision": 0.60, "pattern": 0.40 }`.
7. For each search result: `phase_histogram_norm = histogram_fraction[entry.category]` (0.0 if absent).
8. `compute_fused_score` includes `0.02 * phase_histogram_norm` for each candidate.
9. Decision and pattern entries receive a small positive boost; other categories receive no boost.
10. Top-k returned; decision/pattern entries rank slightly higher than otherwise equal entries in other categories.

### Workflow 2: Cold-Start Session

1. Session registered; no stores yet; histogram is empty.
2. `context_search(query: "...", session_id: "sid-xyz")`.
3. Handler calls `get_category_histogram("sid-xyz")` → empty map.
4. `ServiceSearchParams.category_histogram = None`.
5. All `phase_histogram_norm = 0.0`; fused score computation identical to pre-crt-026.
6. No behavioral difference observed.

### Workflow 3: No Session ID

1. Agent calls `context_search` without `session_id` field (or field is `null`).
2. `ctx.audit_ctx.session_id` is `None`.
3. `ServiceSearchParams.category_histogram = None`, `session_id = None`.
4. Identical to cold-start path — no boost, no error.

### Workflow 4: PreCompact Injection

1. Hook fires `CompactPayload` event with `session_id`.
2. `handle_compact_payload` resolves `category_counts` via `get_category_histogram`.
3. Non-empty: `format_compaction_payload` appends `Recent session activity: decision × 3, pattern × 2`.
4. The block is informational; the receiving agent can use it or ignore it.

---

## Data Flow

```
[context_store handler]
  validate → category_validate → snapshot current_phase → build NewEntry → insert()
  → duplicate guard → record_category_store(sid, category)  ← NEW (FR-04)
  → confidence seeding → record_usage

[context_search handler — MCP]
  parse SearchParams → build_context() → audit_ctx.session_id
  → get_category_histogram(sid)              ← NEW pre-resolution (FR-06)
  → ServiceSearchParams {
       session_id: Option<String>,           ← NEW (FR-05)
       category_histogram: Option<HashMap>,  ← NEW (FR-05)
       ... existing fields ...
    }
  → SearchService::search(params, audit_ctx, caller_id)
    → HNSW(k=20) → try_nli_rerank
    → per-candidate scoring loop:
        FusedScoreInputs {
          phase_histogram_norm: p(entry.category),  ← NEW (FR-08)
          phase_explicit_norm:  0.0,                ← always 0.0 (W3-1 reserved)
          ... existing fields ...
        }
        compute_fused_score(&inputs, &weights)
          = w_nli*nli + w_sim*sim + w_conf*conf + w_coac*coac
          + w_util*util + w_prov*prov
          + w_phase_histogram * phase_histogram_norm  ← NEW (FR-10)
          + w_phase_explicit  * phase_explicit_norm   ← NEW, always 0.0
        final_score = fused * status_penalty
    → sort by final_score → top-k

[context_search handler — UDS]
  HookRequest::ContextSearch { query, session_id, k }
  session_id originates from hook payload field (NOT from audit_ctx)
  → same pre-resolution: get_category_histogram(session_id)  ← NEW (FR-07)
  → ServiceSearchParams constructed identically to MCP path

[handle_compact_payload — UDS PreCompact]
  resolve session_state → get_category_histogram(session_id)  ← NEW (FR-12)
  → format_compaction_payload appends histogram summary block if non-empty
```

---

## Constraints

**C-01: WA-1 dependency.**
`SessionState.current_phase` and `set_current_phase()` are provided by crt-025 (GH #330, complete).
crt-026 reads `current_phase` but does not modify it and does not ship behavior dependent on it
(the explicit phase term is `0.0`).

**C-02: No schema changes.**
`category_counts` is in-memory, per-session state only. No new tables, no migration, no schema
version bump.

**C-03: No new crates.**
All changes are in `crates/unimatrix-server`. Specifically: `infra/session.rs`, `infra/config.rs`,
`services/search.rs`, `mcp/tools.rs`, `uds/listener.rs`.

**C-04: `FusionWeights` sum-invariant.**
The phase boost terms are integrated INTO `compute_fused_score` (OQ-01 resolved). The sum-invariant
in `FusionWeights` doc-comment (`<= 1.0`) must be updated to clarify phase terms are excluded.
With defaults `w_phase_histogram=0.02, w_phase_explicit=0.0`, the total sum is `0.97` — valid,
within `<= 1.0`. The original six-field sum check in `InferenceConfig::validate()` is unchanged.

**C-05: Boost bounded.**
`p(category)` is in `[0.0, 1.0]`; max histogram boost = `0.02 * 1.0 = 0.02` with defaults.
Within WA-0's 0.05 headroom. This prevents a weak-similarity entry from overriding a
high-NLI entry given current weight calibrations (NLI dominant at 0.35).

**C-06: Application order — boost inside `compute_fused_score`, before `status_penalty`.**
`final_score = compute_fused_score(&inputs, &weights) * status_penalty`. The histogram boost
participates in the fused score before the status penalty multiplier is applied. This is consistent
with OQ-01 resolved (integrate into `compute_fused_score`) and SR-09 from the risk assessment.

**C-07: Phase string vocabulary is opaque.**
Unimatrix does not interpret phase strings (WA-1 ADR). The `phase_explicit_norm` field in
`FusedScoreInputs` is always `0.0` in crt-026. The `phase_category_weight(category, phase)`
mapping function is NOT implemented in this feature. W3-1 will learn the relationship from
training data.

**C-08: Hook timeout budget.**
UDS operations under 40ms `HOOK_TIMEOUT`. Histogram summary is a string-format operation on
pre-resolved in-memory data — no I/O, no blocking. Negligible latency impact.

**C-09: Duplicate store guard.**
The histogram must only be incremented after the duplicate check confirms the store is a new entry
(`insert_result.duplicate_of.is_none()`). The Mutex lock-and-increment is synchronous and
fire-and-commit; the window for a concurrent same-entry race within the same session is effectively
zero given lock granularity.

---

## Dependencies

| Dependency | Type | Status |
|---|---|---|
| crt-025 (WA-1, GH #330) | Upstream feature | Complete |
| `rusqlite` / `unimatrix-store` | Storage crate | No changes required |
| `rmcp 0.16.0` | MCP server | No changes required |
| `SessionRegistry` / `SessionState` | `infra/session.rs` | Extended by this feature |
| `ServiceSearchParams` | `services/search.rs` | Extended by this feature |
| `FusedScoreInputs`, `FusionWeights`, `compute_fused_score` | `services/search.rs` | Extended by this feature |
| `InferenceConfig` | `infra/config.rs` | Extended by this feature |
| `format_compaction_payload` / `handle_compact_payload` | `uds/listener.rs` | Extended by this feature |
| W3-1 (GNN relevance function) | Downstream consumer | Uses `w_phase_histogram` as cold-start seed |

---

## NOT in Scope

- Markov model or transition prediction for category sequences
- Phase-annotated co-access edges (W1-5)
- GNN training data production or label generation
- Changes to `context_briefing` behavior
- Changes to `context_lookup` (deterministic, not similarity-ranked)
- Histogram decay or time-weighting of older session stores
- Explicit phase boost behavior (`w_phase_explicit > 0.0`, `phase_category_weight` mapping) — deferred to W3-1
- Changes to the `context_cycle` tool or phase machinery (crt-025, complete)
- Changes to WA-3 (MissedRetrieval) or WA-4 (Proactive Delivery)
- Any new database tables or schema migrations
- Any new workspace crates

---

## Open Questions for Architect

These questions do not block specification delivery. The architect must resolve them in the
IMPLEMENTATION-BRIEF before coding begins.

**OQ-A (from SR-02):** Confirm `InferenceConfig::validate()` permits `sum = 0.97` without
failing any startup check (six-field sum check unchanged at 0.95; phase fields excluded). Resolved
in ARCHITECTURE.md OQ-A: no existing test asserts the exact 0.95 default sum.

**OQ-B (from SR-08):** The UDS search path (`handle_context_search`) receives `session_id` from
`HookRequest::ContextSearch` — the payload field, not `audit_ctx`. Confirm the current field name
in `HookRequest::ContextSearch` and whether any sanitization (like `sanitize_session_id`) is already
applied before `get_category_histogram` is called.

**OQ-C (from SR-07):** The pre-resolution pattern (handler resolves histogram, passes via `ServiceSearchParams`)
is sufficient for crt-026 and is preferred for `SearchService` isolation. Confirm whether WA-4a
(proactive injection — no user query, session context IS the retrieval anchor) will need
`Arc<SessionRegistry>` on `SearchService` or can also use a pre-resolution approach. This is a
forward-compatibility flag only; no code change required in crt-026.

**OQ-D (from SR-09):** Confirm `status_penalty` is applied AFTER `compute_fused_score` returns
(i.e., `final_score = compute_fused_score(...) * status_penalty`), so the histogram boost participates
in the pre-penalty fused score. Current code at line 366-367 of `search.rs` suggests this is already
the case; architect should confirm no other penalty path exists.

---

## Knowledge Stewardship

Queried: `/uni-query-patterns` for session context ranking, ServiceSearchParams, FusionWeights,
affinity boost architecture — found entries #3157 (pre-resolution pattern resolved as OQ-02) and
#3156 (boost inside `compute_fused_score` resolved as OQ-01). Both confirmed the SCOPE.md resolved
decisions. No conflicting conventions found.
