# crt-026: WA-2 Session Context Enrichment — Pseudocode Overview

GH Issue: #341

---

## Feature Summary

Adds a per-session category histogram to `SessionState` and wires it into
`compute_fused_score` as a seventh dimension (`phase_histogram_norm`). Sessions
without prior stores receive identical scores to the pre-crt-026 pipeline (cold-start
safe). The histogram summary is also appended to the UDS `CompactPayload` output.

---

## Components Involved

| Component | File | Role |
|-----------|------|------|
| `SessionState` + `SessionRegistry` | `infra/session.rs` | Accumulate and expose category histogram |
| `context_store` handler — histogram recording | `mcp/tools.rs` | Increment histogram on non-duplicate store |
| `ServiceSearchParams` new fields | `services/search.rs` | Carry pre-resolved histogram to scoring loop |
| `context_search` handler — pre-resolution | `mcp/tools.rs` | Snapshot histogram before any `await` |
| `FusedScoreInputs` / `FusionWeights` / `compute_fused_score` | `services/search.rs` | Apply histogram affinity as seventh dimension |
| `InferenceConfig` new fields | `infra/config.rs` | Expose `w_phase_histogram` and `w_phase_explicit` as config |
| `handle_context_search` (UDS) | `uds/listener.rs` | Same histogram pre-resolution for hook-driven searches |
| `handle_compact_payload` + `format_compaction_payload` | `uds/listener.rs` | Append histogram summary to CompactPayload output |

---

## Shared Types Introduced or Modified

### `SessionState` — new field

```
category_counts: HashMap<String, u32>
    Initialized to HashMap::new() in register_session.
    In-memory only; never persisted.
    Mutated by: record_category_store (SessionRegistry method).
    Read by: get_category_histogram (SessionRegistry method).
```

### `ServiceSearchParams` — two new fields

```
session_id: Option<String>
    The session identifier for logging/tracing. No logic.

category_histogram: Option<HashMap<String, u32>>
    Pre-resolved clone of SessionState.category_counts.
    None when: session_id is None, session unregistered, or histogram is empty.
    The handler maps is_empty() result → None (not Some(empty_map)).
```

### `FusedScoreInputs` — two new fields (replace WA-2 stub at line 55)

```
phase_histogram_norm: f64    -- p(entry.category) from session histogram; [0.0, 1.0]
phase_explicit_norm:  f64    -- always 0.0 in crt-026; W3-1 reserved (ADR-003)
```

### `FusionWeights` — two new fields (replace WA-2 stub at line 89)

```
w_phase_histogram: f64    -- default 0.02 (ASS-028 calibrated, full session signal budget)
w_phase_explicit:  f64    -- default 0.0  (W3-1 placeholder; ADR-003)
```

Invariant doc-comment updated from `sum of six <= 1.0` to:
`sum of six core terms <= 1.0; w_phase_histogram and w_phase_explicit are additive
terms excluded from this constraint`.

### `InferenceConfig` — two new fields

```
w_phase_explicit: f64    -- serde default 0.0
w_phase_histogram: f64   -- serde default 0.02
```

---

## Data Flow Between Components

```
[context_store — mcp/tools.rs]
  on insert_result.duplicate_of.is_none() AND session_id is Some:
    → session_registry.record_category_store(sid, &params.category)
         → SessionState.category_counts[category] += 1

[context_search — mcp/tools.rs]
  step 4a (NEW, before constructing ServiceSearchParams, before any await):
    → get_category_histogram(sid) → HashMap<String, u32> clone or empty
    → if empty → category_histogram = None; else category_histogram = Some(histogram)
    → ServiceSearchParams { session_id, category_histogram, ...existing... }
  step 4b (unchanged):
    → SearchService::search(service_params, audit_ctx, caller_id)

[SearchService::search — services/search.rs]
  before scoring loop (once):
    category_histogram = params.category_histogram.as_ref()
    total = histogram.map(|h| h.values().sum()).unwrap_or(0)
  per-candidate (i, entry, sim):
    phase_histogram_norm = if total > 0 {
        histogram.and_then(|h| h.get(&entry.category)).copied().unwrap_or(0) as f64 / total as f64
    } else { 0.0 }
    FusedScoreInputs { phase_histogram_norm, phase_explicit_norm: 0.0, ...existing... }
    fused = compute_fused_score(&inputs, &effective_weights)
    final_score = fused * penalty    -- penalty applied after fused score (OQ-D confirmed)

[handle_context_search — uds/listener.rs]
  after sanitize_session_id check (lines 796-803) and BEFORE constructing ServiceSearchParams:
    → same pre-resolution as MCP path
    → ServiceSearchParams { session_id, category_histogram, ...existing... }

[handle_compact_payload — uds/listener.rs]
  after resolving session_state:
    → category_histogram = session_registry.get_category_histogram(session_id)
    → pass category_histogram to format_compaction_payload
  format_compaction_payload:
    if category_histogram non-empty:
      append "Recent session activity: {cat} × {n}, ..." (top-5 by count, > 0 only)
    else: omit block entirely
```

---

## Sequencing Constraints (Build Order)

### Wave 1 — Parallel (no inter-dependency)

| Component | Pseudocode File | Why Wave 1 |
|-----------|-----------------|------------|
| `session.rs` | `session.md` | Foundation — provides `record_category_store` and `get_category_histogram` |
| `services/search.rs` (params + scoring structs + fused score) | `search-params.md`, `fused-score.md` | Foundation — `ServiceSearchParams`, `FusedScoreInputs`, `FusionWeights`, `compute_fused_score` |
| `infra/config.rs` | `config.md` | Foundation — `InferenceConfig` new fields, `FusionWeights::from_config` reads them |

Wave 1 components have no dependency on each other. All three can be implemented
and committed in parallel. The test suite must compile after Wave 1 (existing struct
literal sites need updating for `ServiceSearchParams`, `FusedScoreInputs`, `FusionWeights`).

### Wave 2 — Parallel (depends on Wave 1 being committed)

| Component | Pseudocode File | Why Wave 2 |
|-----------|-----------------|------------|
| `mcp/tools.rs` — store handler + search handler | `store-handler.md`, `search-handler.md` | Calls `record_category_store` and `get_category_histogram` from Wave 1 session.rs; populates `category_histogram` field from Wave 1 search-params |
| `uds/listener.rs` — search + compact | `uds.md` | Same dependencies as tools.rs; also calls `format_compaction_payload` which gains the histogram parameter |

---

## Key Invariants (Cross-Cutting)

- **Cold-start safe**: `category_histogram = None` → `phase_histogram_norm = 0.0` for
  all candidates → `compute_fused_score` output bit-for-bit identical to pre-crt-026.
- **Duplicate-store guard**: `record_category_store` called ONLY when
  `insert_result.duplicate_of.is_none()`.
- **Pre-resolution before await**: `get_category_histogram` in both MCP and UDS handlers
  must occur before the first `await` point in the function (SR-07 snapshot pattern).
- **No new crates, no schema changes**: all changes in `crates/unimatrix-server`.
- **WA-2 extension stubs replaced**: no `WA-2 extension:` comment may remain in `search.rs`.
- **phase_explicit_norm always 0.0**: comment citing ADR-003 must accompany the assignment.
- **FusionWeights::effective() NLI-absent denominator**: five terms only
  (`w_sim + w_conf + w_coac + w_util + w_prov`). Phase fields are passed through unchanged.
