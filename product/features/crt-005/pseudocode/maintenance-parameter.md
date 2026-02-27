# Pseudocode: C7 Maintenance Parameter

## Purpose

Add `maintain: Option<bool>` to StatusParams. Controls whether context_status performs write operations (confidence refresh, graph compaction, co-access cleanup).

## Files Modified

- `crates/unimatrix-server/src/tools.rs` -- StatusParams struct + handler logic

## StatusParams Extension

```
pub struct StatusParams {
    pub topic: Option<String>,
    pub category: Option<String>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
    pub check_embeddings: Option<bool>,
    // NEW (crt-005):
    pub maintain: Option<bool>,   // Default: false (read-only). true = run maintenance.
}
```

The `maintain` field must also be extracted from the MCP tool arguments in the same way as other Optional fields.

## Maintain Resolution Logic

```
let maintain_enabled = params.maintain.unwrap_or(false);
```

ADR-002 (UPDATED): Default is false. Status is read-only by default. Writes only when `maintain: true`.

## Handler Gating

The maintain flag gates three write operations:

1. **Confidence refresh** (C5): Only when maintain_enabled
2. **Co-access stale pair cleanup** (existing): Only when maintain_enabled
3. **Graph compaction** (C8): Only when maintain_enabled

Reads (dimension scores, lambda computation, recommendations) always run regardless of maintain.

```
// In context_status handler:

// Dimension scores: ALWAYS computed (read-only)
freshness_score = confidence_freshness_score(entries, now, threshold)
graph_score = graph_quality_score(stale_count, point_count)
embed_score = if check_embeddings { Some(embedding_consistency_score(...)) } else { None }
contra_score = contradiction_density_score(quarantined, active)

// Confidence refresh: ONLY when maintain=true (C5)
if maintain_enabled:
    // ... refresh stale entries ...

// Co-access cleanup: ONLY when maintain=true (existing behavior change)
if maintain_enabled:
    // ... clean stale co-access pairs ...

// Graph compaction: ONLY when maintain=true (C8)
if maintain_enabled && stale_ratio > threshold:
    // ... trigger compaction ...

// Lambda + recommendations: ALWAYS computed (read-only)
lambda = compute_lambda(...)
recs = generate_recommendations(...)
```

## MCP Tool Schema Update

The `maintain` parameter must appear in the tool's parameter schema so agents can pass it:

```
{
    "name": "maintain",
    "description": "Set to true to run maintenance writes (confidence refresh, graph compaction). Default: false (read-only diagnostics).",
    "required": false,
    "schema": { "type": "boolean" }
}
```

Check the existing tool schema registration pattern in tools.rs to add this correctly.

## Key Test Scenarios

1. maintain=false: confidence_refreshed_count=0, graph_compacted=false (R-07, AC-09)
2. maintain=false: dimension scores still computed (R-07)
3. maintain=true + stale entries: refresh runs (AC-09)
4. maintain=true + high stale ratio: compaction triggers
5. maintain absent (default): same as false (AC-09)
6. maintain=false: co-access cleanup skipped
7. maintain=false: contradiction scanning still runs (reads, not writes)
