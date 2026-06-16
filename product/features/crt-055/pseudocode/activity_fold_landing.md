# Component 6 — Activity-fold landing (read-before-purge, width conversion, JSON)

**Crate**: `unimatrix-server` (review pipeline) + read of crt-054's `ActivitySnapshot`
**Files**: `tools.rs` review pipeline (the landing block); consumes `infra/session.rs:560` `activity_snapshots_for_feature`
**ADRs**: ADR-007 (#5042), ADR-008 (#5043), ADR-003 (#5046) | **Risks**: R-03 (read-after-purge), R-04 (held-route zero), R-09 (int width), R-11 (leak) | **Wave**: 3

## Purpose

Read each held session's `ActivitySnapshot` BEFORE the crt-052 hold purge, sum across the cycle's sessions, convert producer widths (`u64`/`u32`) to `i64` checked/saturating, and build `signal_class_counts_json`. Lands `transcript_*` columns + the JSON map. Content-free (integers only) — leak gate intact.

## Constraints honored

- **Read-before-purge (Constraint 4 / R-03)**: this block runs STRICTLY before `purge_cycle_transcripts` (ordering enforced + asserted in Component 9 / tests).
- **Width conversion (R-09)**: `u64`/`u32` → `i64` checked/saturating; saturate-and-warn, never wrap/panic.
- **Fixed catalog indices (ADR-008)**: `class_counts[0] = error`, `[1] = refusal`. Read by fixed index.
- **Leak gate (Constraint 5)**: only integer counters + a `class_name→count` JSON map; no transcript bytes.
- **Coarse/directional (Constraint 6)**: these counts are presentation-marked directional by Component 7; this component only lands the integers.

## 6a. The landing block (in the review pipeline, before purge)

```
fn land_activity_fold(registry, feature_cycle, signal_catalog) -> FoldLanding:
    // Read crt-054's collector — undeclared sessions are ABSENT (not zero); a present
    // session with a zero buffer appears with a zero snapshot (measured zero).
    snaps: Vec<(String /*sid*/, ActivitySnapshot)> =
        registry.activity_snapshots_for_feature(feature_cycle)   // infra/session.rs:560

    any_declared_fold = !snaps.is_empty()    // ≥1 declared session contributed → fold available

    // Sum across the cycle's sessions, saturating in producer widths, then convert to i64.
    bytes_total_u64: u64 = 0
    delta_count_u64: u64 = 0
    class_sums_u64: [u64; MAX_SIGNAL_CLASSES] = [0; 16]
    for (_sid, snap) in snaps:
        bytes_total_u64 = bytes_total_u64.saturating_add(snap.bytes_total)
        delta_count_u64 = delta_count_u64.saturating_add(snap.delta_count as u64)  // widening u32→u64
        for i in 0..MAX_SIGNAL_CLASSES:
            class_sums_u64[i] = class_sums_u64[i].saturating_add(snap.class_counts[i] as u64)

    return FoldLanding {
        available: any_declared_fold,
        transcript_bytes_total:  u64_to_i64_saturating(bytes_total_u64),
        transcript_delta_count:  u64_to_i64_saturating(delta_count_u64),
        transcript_error_count:  u64_to_i64_saturating(class_sums_u64[0]),    // catalog[0] = error
        transcript_refusal_count:u64_to_i64_saturating(class_sums_u64[1]),    // catalog[1] = refusal
        signal_class_counts_json: build_signal_json(class_sums_u64, signal_catalog),
    }
```

`u64_to_i64_saturating(v)` = `if v > i64::MAX as u64 { warn!; i64::MAX } else { v as i64 }`. Practically impossible to hit, but never wraps (R-09).

## 6b. signal_class_counts_json builder (forward-compatible map)

```
fn build_signal_json(class_sums_u64: [u64; 16], catalog: &[ClassName]) -> String:
    // Map class_name → summed count for every ENABLED class in catalog order.
    // catalog supplies the names for the active indices (crt-054 [transcript_signals]);
    // crt-055 consumes the configured order, reads counts by index.
    map = ordered map<String, i64>
    for (idx, name) in catalog.enumerate():    // only enabled classes, idx < class_count
        map[name] = u64_to_i64_saturating(class_sums_u64[idx])
    // Serialize via a REAL JSON serializer (serde_json) — NEVER string concatenation
    // (security: class_name from config; serde escapes it). Empty catalog → "{}".
    return serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
```

Forward-compatibility (NFR-06): classes added beyond error/refusal land in the JSON map with no new column or migration. `error`/`refusal` also have dedicated columns (fixed indices) for cross-cycle baseline queries.

OPEN-Q: confirm how crt-055 obtains the catalog `class_name` list (the `[transcript_signals]` config the producer compiled). crt-054 owns `SignatureScanner`; the names must be reachable at review (config read or a producer-exposed accessor). If only indices are available, fall back to canonical names `{"error","refusal", ...}` for v1 indices and confirm at implementation. Do NOT invent names beyond the pinned v1 catalog.

## 6c. Wiring into CycleAggregates

```
landing = land_activity_fold(registry, feature_cycle, catalog)   // BEFORE purge (Component 9)
aggregates.transcript_bytes_total   = landing.transcript_bytes_total
aggregates.transcript_delta_count   = landing.transcript_delta_count
aggregates.transcript_error_count   = landing.transcript_error_count
aggregates.transcript_refusal_count = landing.transcript_refusal_count
aggregates.signal_class_counts_json = landing.signal_class_counts_json
availability.transcript_fold_available = landing.available    // Component 7
```

## Data flow

- IN: `SessionRegistry` (collector), `feature_cycle`, signal catalog (names).
- crt-054 boundary: `Vec<(String, ActivitySnapshot)>` (u64/u32 widths) — READ ONLY.
- OUT: 4 `i64` + 1 `String` on `CycleAggregates` + `transcript_fold_available` flag.

## Error handling

- Collector is infallible (lock poison degrades to empty buffer per #4764) — never panics, never drops.
- Empty `snaps` (undeclared-only cycle / no held activity) → all transcript columns 0, `available=false` → Component 7 renders "unavailable", never a measured 0 (R-04).
- One undeclared session among valid declared sessions does NOT zero the valid sessions' fold — the collector simply omits the undeclared one (per-session sum, not a cycle-wide flag).

## Key test scenarios

- Known fold → each column equals the summed snapshot field; JSON map matches the class catalog (AC-07).
- **Read-before-purge ordering (AC-08, R-03)**: the `activity_snapshots_for_feature` call site strictly precedes `purge_cycle_transcripts`; an inversion test (purge first) zeroes the columns (proving the ordering is load-bearing).
- **Silent-zero regression guard (AC-09, R-04)**: a representative TS-client cycle with held activity → fold source non-empty, `transcript_*` columns non-zero.
- Undeclared-only cycle → transcript metrics "unavailable", never 0 (R-04).
- Near-`u64::MAX` / large `u32` fold values → persisted `i64` saturated, never wrapped (AC-14, R-09).
- `signal_class_counts_json` serialized via serde (no concatenation); a class_name with JSON-special chars round-trips safely; empty catalog → "{}".
- No content field / no token-named field introduced (AC-10, AC-19, R-11, R-13).
