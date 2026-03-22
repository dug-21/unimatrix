# crt-026: WA-2 Session Context Enrichment — Architecture

GH Issue: #341

---

## System Overview

crt-026 wires per-session category histogram data into the search ranking pipeline.
`context_search` is currently session-blind: every query is scored by the same six-term
fused formula (`w_nli·nli + w_sim·sim + w_conf·conf + w_coac·coac + w_util·util +
w_prov·prov`) regardless of what the current session has been producing. This feature
adds a seventh dimension — histogram affinity — as a first-class term inside
`compute_fused_score`, and surfaces the histogram summary in the UDS `CompactPayload`
synthesis output.

The feature builds directly on top of:
- **crt-025 (WA-1)**: `SessionState.current_phase`, `set_current_phase()`, and the
  SR-07 pre-snapshot pattern are in place. crt-026 does not touch them.
- **crt-024 (WA-0)**: `FusedScoreInputs`, `FusionWeights`, `compute_fused_score`, and
  the WA-2 extension stubs are the integration contract for this feature.

All changes are confined to `crates/unimatrix-server`. No new crates, no schema changes,
no migration.

---

## Component Breakdown

### Component 1: `SessionState.category_counts` + `SessionRegistry` methods
**File**: `crates/unimatrix-server/src/infra/session.rs`

Adds `category_counts: HashMap<String, u32>` to `SessionState`. Initialized to
`HashMap::new()` in `register_session`. Two new public methods on `SessionRegistry`:

- `record_category_store(&self, session_id: &str, category: &str)` — increments
  `category_counts[category]` by 1 under the existing Mutex. Silent no-op for
  unregistered sessions. Lock hold is microseconds — same contract as `record_injection`.
- `get_category_histogram(&self, session_id: &str) -> HashMap<String, u32>` — returns a
  clone of `category_counts`, or an empty map if not registered. This is the sole read
  path for the histogram.

**Responsibility boundary**: Session state accumulation and read. No ranking logic here.

### Component 2: `context_store` handler — histogram recording
**File**: `crates/unimatrix-server/src/mcp/tools.rs`

After the duplicate guard (`insert_result.duplicate_of.is_some()`, step 7), before
confidence seeding (step 8):

```rust
// crt-026: accumulate category histogram for session affinity boost (WA-2)
if let Some(ref sid) = ctx.audit_ctx.session_id {
    self.session_registry.record_category_store(sid, &params.category);
}
```

A duplicate store (where `duplicate_of.is_some()`) must NOT increment the histogram.
Follows the `if let Some(ref sid)` guard pattern used by `record_injection`.

### Component 3: `ServiceSearchParams` — data carrier fields
**File**: `crates/unimatrix-server/src/services/search.rs`

Two new fields on `ServiceSearchParams`:
- `session_id: Option<String>` — session identifier, for logging/tracing
- `category_histogram: Option<HashMap<String, u32>>` — pre-resolved histogram or `None`
  when session has no histogram data (empty map is mapped to `None` by the handler)

No logic belongs in `ServiceSearchParams`; it is a data carrier only.

### Component 4: `context_search` handler — pre-resolution and threading
**File**: `crates/unimatrix-server/src/mcp/tools.rs`

Before constructing `ServiceSearchParams`, in step 4 of the `context_search` handler:

```rust
// crt-026: pre-resolve session histogram (WA-2, SR-07 snapshot pattern)
let category_histogram: Option<HashMap<String, u32>> =
    ctx.audit_ctx.session_id.as_deref().and_then(|sid| {
        let h = self.session_registry.get_category_histogram(sid);
        if h.is_empty() { None } else { Some(h) }
    });
```

Then in `ServiceSearchParams` construction:
```rust
session_id: ctx.audit_ctx.session_id.clone(),
category_histogram,
```

The pre-resolution follows the crt-025 SR-07 snapshot pattern: session state is read
once synchronously before any `await` points, eliminating races with concurrent session
mutations.

### Component 5: `FusedScoreInputs` / `FusionWeights` / `compute_fused_score` — seventh dimension
**File**: `crates/unimatrix-server/src/services/search.rs`

Three targeted extensions to the WA-2 extension stubs:

**`FusedScoreInputs`** gains two new fields (replacing the stub comment at line 55):
- `phase_histogram_norm: f64` — `p(entry.category)` from session histogram, in `[0.0, 1.0]`
- `phase_explicit_norm: f64` — always `0.0` in crt-026 (W3-1 reserved placeholder)

**`FusionWeights`** gains two new fields (replacing the stub comment at line 89):
- `w_phase_histogram: f64` — default `0.02`
- `w_phase_explicit: f64` — default `0.0`

`FusionWeights::from_config()` reads both from `InferenceConfig`. The `effective()` method
is extended to pass through both new weights unchanged when NLI is active, and to exclude
both from the NLI-absent re-normalization denominator (they are not part of the six-term
sum; they are additive terms that do not participate in the `<= 1.0` constraint).

**`compute_fused_score`** gains two additional terms (replacing the stub at line 179):
```rust
+ weights.w_phase_histogram * inputs.phase_histogram_norm
+ weights.w_phase_explicit  * inputs.phase_explicit_norm  // always 0.0 in crt-026
```

**Scoring loop** — per-candidate `FusedScoreInputs` construction:
```rust
let total: u32 = category_histogram.values().sum();
let phase_histogram_norm = if total > 0 {
    category_histogram.get(&entry.category).copied().unwrap_or(0) as f64 / total as f64
} else {
    0.0
};
FusedScoreInputs {
    phase_histogram_norm,
    phase_explicit_norm: 0.0,  // W3-1 reserved
    // ... existing fields unchanged ...
}
```

The `category_histogram` is carried in from `ServiceSearchParams` and is resolved once
before the scoring loop begins — no per-candidate registry access.

### Component 6: `InferenceConfig` — two new config fields
**File**: `crates/unimatrix-server/src/infra/config.rs`

Two new fields added to `InferenceConfig` following the existing `default_w_*` serde
pattern:

```rust
#[serde(default = "default_w_phase_explicit")]
pub w_phase_explicit: f64,  // default 0.0

#[serde(default = "default_w_phase_histogram")]
pub w_phase_histogram: f64, // default 0.02
```

`InferenceConfig::validate()` receives per-field range checks for both new fields
(`[0.0, 1.0]` invariant). The existing six-weight sum check (`w_sim + w_nli + w_conf +
w_coac + w_util + w_prov <= 1.0`) is NOT modified — the phase fields are additive terms
outside the sum-constraint (see ADR-004 and OQ-A resolution below).

The `InferenceConfig::default()` struct literal must be extended with `..Default::default()`
or explicit `w_phase_explicit: 0.0, w_phase_histogram: 0.02` (see #2730 pattern).

### Component 7: UDS `handle_context_search` — session_id threading
**File**: `crates/unimatrix-server/src/uds/listener.rs`

`handle_context_search` already receives `session_id: Option<String>` from the
`HookRequest::ContextSearch` payload field. `sanitize_session_id` is already called on
this value before any session registry access (confirmed in listener.rs lines 796-803).

The pre-resolution is added after the existing sanitize check:

```rust
// crt-026: pre-resolve session histogram for histogram affinity boost (WA-2)
let category_histogram: Option<HashMap<String, u32>> =
    session_id.as_deref().and_then(|sid| {
        let h = session_registry.get_category_histogram(sid);
        if h.is_empty() { None } else { Some(h) }
    });
```

Then `ServiceSearchParams` is extended with both new fields, identical to the MCP path.
This ensures the histogram boost fires on hook-driven searches (OQ-04 / OQ-B resolved).

### Component 8: UDS `handle_compact_payload` — histogram summary
**File**: `crates/unimatrix-server/src/uds/listener.rs`

In `handle_compact_payload`, after resolving `session_state`, extract the category histogram
and pass it to `format_compaction_payload`. When the histogram is non-empty,
`format_compaction_payload` appends:

```
Recent session activity: decision × 3, pattern × 2
```

Format rules:
- Emit only categories with `count > 0`
- Sort by count descending, cap at top-5
- Omit entirely when histogram is empty (no blank line, no header)
- Must fit within `MAX_INJECTION_BYTES` budget (< 100 bytes for typical sessions)

---

## Component Interactions

```
context_store handler (mcp/tools.rs)
  success + non-duplicate
  └── SessionRegistry.record_category_store(sid, category)
        └── SessionState.category_counts[category] += 1

context_search handler (mcp/tools.rs)
  step 4 — pre-resolution (before any await)
  └── SessionRegistry.get_category_histogram(sid)
        └── returns HashMap<String, u32> clone (or empty → None)
  └── ServiceSearchParams { session_id, category_histogram, ...existing... }
  └── SearchService::search(params, audit_ctx, caller_id)
        └── scoring loop: per-candidate FusedScoreInputs
              └── phase_histogram_norm = p(entry.category) from params.category_histogram
              └── compute_fused_score(&inputs, &effective_weights)
                    += w_phase_histogram * phase_histogram_norm
                    += w_phase_explicit  * 0.0  (always zero)
              └── final_score = fused * penalty  ← status_penalty applied here, after fused

handle_context_search (uds/listener.rs)
  session_id from HookRequest::ContextSearch payload field
  sanitize_session_id already applied (lines 796-803)
  └── same pre-resolution as MCP path
  └── same ServiceSearchParams extension

handle_compact_payload (uds/listener.rs)
  └── SessionRegistry.get_category_histogram(session_id)
        └── format_compaction_payload appends summary block when non-empty

InferenceConfig (infra/config.rs)
  └── w_phase_explicit: f64 = 0.0   (new, serde(default))
  └── w_phase_histogram: f64 = 0.02 (new, serde(default)) ← full session signal budget (ADR-004)
  └── validate() — per-field range check [0.0, 1.0] for both new fields
                 — existing six-weight sum check NOT modified

FusionWeights::from_config() (services/search.rs)
  └── reads w_phase_explicit and w_phase_histogram from InferenceConfig
```

---

## Technology Decisions

See individual ADR files for full rationale.

| Decision | Choice | ADR |
|----------|--------|-----|
| Boost integration point | Inside `compute_fused_score` as first-class dimension | ADR-001 |
| Session registry access in search | Pre-resolve in handler, pass via `ServiceSearchParams` | ADR-002 |
| `w_phase_explicit` in crt-026 | Ship at `0.0` as W3-1 placeholder; no mapping function | ADR-003 |
| Weight sum invariant | No rebalancing; w_phase_histogram=0.02 carries full session signal budget; sum goes 0.95 → 0.97, within `<= 1.0` | ADR-004 |

---

## Integration Points

### Upstream (dependencies, unchanged by crt-026)

- **crt-025 (WA-1)**: `SessionState.current_phase` and `set_current_phase()` are read-only
  in crt-026. The SR-07 pre-snapshot pattern is the model for the histogram snapshot.
- **crt-024 (WA-0)**: `FusedScoreInputs`, `FusionWeights`, `compute_fused_score` stubs at
  lines 55, 89, 179 of `search.rs` are the integration contract for this feature.

### Downstream (consumers of new capabilities)

- **W3-1 (GNN)**: `phase_histogram_norm` and `w_phase_histogram` are stable, named,
  learnable dimensions. W3-1 initializes from `w_phase_histogram=0.02` (ASS-028 calibrated value) and refines from
  there. The field names must not be renamed or removed without a W3-1 migration plan.
- **WA-4a (proactive injection)**: the pre-resolution pattern (OQ-C, see below) is
  sufficient for crt-026 but may not be reusable for WA-4a, which resolves candidates
  without a user query and may need `Arc<SessionRegistry>` on `SearchService`. This is
  flagged as a forward-compatibility note in ADR-002.

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `SessionState.category_counts` | `HashMap<String, u32>` | `infra/session.rs` — NEW field |
| `SessionRegistry::record_category_store` | `fn(&self, session_id: &str, category: &str)` | `infra/session.rs` — NEW method |
| `SessionRegistry::get_category_histogram` | `fn(&self, session_id: &str) -> HashMap<String, u32>` | `infra/session.rs` — NEW method |
| `ServiceSearchParams.session_id` | `Option<String>` | `services/search.rs` — NEW field |
| `ServiceSearchParams.category_histogram` | `Option<HashMap<String, u32>>` | `services/search.rs` — NEW field |
| `FusedScoreInputs.phase_histogram_norm` | `f64` in `[0.0, 1.0]` | `services/search.rs` — NEW field (replaces WA-2 stub) |
| `FusedScoreInputs.phase_explicit_norm` | `f64`, always `0.0` in crt-026 | `services/search.rs` — NEW field (W3-1 reserved) |
| `FusionWeights.w_phase_histogram` | `f64`, default `0.02` | `services/search.rs` — NEW field (replaces WA-2 stub) |
| `FusionWeights.w_phase_explicit` | `f64`, default `0.0` | `services/search.rs` — NEW field |
| `InferenceConfig.w_phase_explicit` | `f64`, serde default `0.0` | `infra/config.rs` — NEW field |
| `InferenceConfig.w_phase_histogram` | `f64`, serde default `0.02` | `infra/config.rs` — NEW field |
| `HookRequest::ContextSearch.session_id` | `Option<String>`, `#[serde(default)]` | `unimatrix-engine/src/wire.rs` — existing, no change |
| `compute_fused_score` final formula | `w_phase_histogram * phase_histogram_norm + w_phase_explicit * phase_explicit_norm` | `services/search.rs` — extends existing function |
| `format_compaction_payload` histogram block | `"Recent session activity: {cat} × {n}, ..."` appended when non-empty | `uds/listener.rs` — conditional append |

---

## Open Questions Resolution

### OQ-A: Does `InferenceConfig::validate()` accept `sum = 0.97`?

**Confirmed: Yes.** The existing sum check at `config.rs` line 597-600 computes
`w_sim + w_nli + w_conf + w_coac + w_util + w_prov` — the six original fields only — and
tests `> 1.0`. Adding `w_phase_histogram=0.02` does NOT enter this sum; the new fields
are outside the six-weight constraint. The sum of the original six remains `0.95`, which
passes the `> 1.0` guard. Total sum including phase fields: `0.97`.

The doc-comment on `FusionWeights` must be updated to clarify: `sum of six core terms <= 1.0;
w_phase_histogram and w_phase_explicit are additive terms excluded from this constraint`.
There is no test asserting `sum == 0.95` exactly against the
constant. The test `test_fusion_weights_effective_nli_active_headroom_weight_preserved`
uses a manually constructed `FusionWeights` that sums to `0.90` and asserts that
`effective(true)` does NOT re-normalize — that test remains valid because it does not
assert a specific total weight value for the struct defaults.

**Action required**: the `FusionWeights` struct's invariant doc-comment must be updated
from `<= 1.0 (sum of six)` to `<= 1.0 (sum of six core terms; phase terms are additive
and excluded from this constraint)`. No existing tests assert the exact `0.95` default sum.

### OQ-B: `HookRequest::ContextSearch` — field name and sanitization order

**Confirmed.** The field is `session_id: Option<String>` with `#[serde(default)]` in
`unimatrix-engine/src/wire.rs`. In `handle_context_search` (listener.rs lines 781-838),
`sanitize_session_id` is called on the value at lines 796-803 **before** any session
registry access. The histogram pre-resolution must be placed **after** the sanitize check
and **before** `ServiceSearchParams` construction — this is already the correct ordering
given the function structure. No additional sanitization is needed for `get_category_histogram`
because the value was already validated before reaching that point.

### OQ-C: WA-4a forward-compatibility with pre-resolution pattern

**Flagged as forward-compatibility risk.** The pre-resolution pattern (handler resolves
histogram, passes via `ServiceSearchParams`) is clean and correct for crt-026. However,
WA-4a (proactive injection) resolves candidates without a user query — the session context
IS the retrieval anchor, not a pre-filter. WA-4a will likely need to initiate a search
from within a context that does not have a tool handler on the call stack, making
pre-resolution in a handler impossible. WA-4a will in all likelihood require
`Arc<SessionRegistry>` on `SearchService` (reopening ADR-002's rejected option). This
does not require any code change in crt-026 — the ADR-002 decision is correct for this
feature. WA-4a must re-evaluate and supersede ADR-002 if needed.

### OQ-D: `status_penalty` application order

**Confirmed.** At `search.rs` line 798-800, the application is:
```rust
let fused = compute_fused_score(&inputs, &effective_weights);
let penalty = penalty_map.get(&entry.id).copied().unwrap_or(1.0);
let final_score = fused * penalty;
```
`status_penalty` is applied AFTER `compute_fused_score` returns. The histogram boost
(inside `compute_fused_score`) therefore participates in the pre-penalty fused score, which
is the correct behavior per C-06 of the specification. This matches SR-09's preferred
ordering: `(fused_score_including_histogram) * status_penalty`.

---

## Data Flow Summary

```
[context_store]
  validate → category_validate → snapshot current_phase → build NewEntry → insert()
  → duplicate guard (is_some? skip) → record_category_store(sid, category)  ← NEW
  → confidence seeding → record_usage

[context_search — MCP]
  parse → build_context → audit_ctx.session_id
  → get_category_histogram(sid) → category_histogram: Option<HashMap>  ← NEW pre-resolve
  → ServiceSearchParams { session_id, category_histogram, ...existing... }
  → SearchService::search
    → embed → HNSW(k=20 or k) → try_nli_rerank
    → per-candidate scoring:
        phase_histogram_norm = p(entry.category)  ← NEW
        compute_fused_score  ← extended with +w_phase_histogram*p + 0.0
        final_score = fused * status_penalty
    → sort DESC → top-k

[context_search — UDS]
  HookRequest::ContextSearch { session_id, query, k, ... }
  sanitize_session_id(session_id)  ← already in place
  → get_category_histogram(sid)  ← NEW pre-resolve
  → same ServiceSearchParams construction → same SearchService path

[handle_compact_payload — UDS PreCompact]
  resolve session_state → get_category_histogram(session_id)  ← NEW
  → format_compaction_payload appends histogram summary block if non-empty
```

---

## Constraints Confirmed

- **No schema changes**: `category_counts` is in-memory per-session state only.
- **No new crates**: all changes in `crates/unimatrix-server`.
- **Cold-start safe**: empty histogram → `category_histogram = None` → all
  `phase_histogram_norm = 0.0` → `compute_fused_score` identical to pre-crt-026.
- **Boost bounded**: max histogram boost = `0.02 * 1.0 = 0.02` with defaults (p=1.0 concentration).
  Within `0.05` WA-0 headroom. When W3-1 enables `w_phase_explicit`, W3-1 will learn the
  appropriate split between the two terms — expected to reduce `w_phase_histogram` as
  `w_phase_explicit` gains weight from training data.
- **Hook timeout budget**: histogram summary is string formatting on pre-resolved in-memory
  data. Negligible latency; no I/O path within `HOOK_TIMEOUT = 40ms`.
- **`FusionWeights::effective()` NLI-absent path**: the re-normalization denominator
  (`w_sim + w_conf + w_coac + w_util + w_prov`) must NOT include `w_phase_histogram` or
  `w_phase_explicit`. These are additive terms outside the six-term normalization group.
  The `effective()` method returns the new fields unchanged (pass-through) in both the NLI
  active and NLI absent paths.
