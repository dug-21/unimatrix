# Component 4: Provenance Boost — Pseudocode

## Files Modified

- `crates/unimatrix-engine/src/confidence.rs` — PROVENANCE_BOOST constant
- `crates/unimatrix-server/src/tools.rs` — Search re-ranking (MCP path)
- `crates/unimatrix-server/src/uds_listener.rs` — Search re-ranking (UDS/hook path)

## 1. Constant Definition (confidence.rs)

```pseudo
/// Query-time boost for `lesson-learned` category entries (col-010b).
/// Applied in search re-ranking alongside co-access affinity.
/// Does NOT modify the stored confidence formula invariant (0.92).
pub const PROVENANCE_BOOST: f64 = 0.02;
```

## 2. Application Site 1: tools.rs (MCP context_search)

### 2a. Initial Sort (step 9b)

```pseudo
// Current:
results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
    let score_a = rerank_score(*sim_a, entry_a.confidence);
    let score_b = rerank_score(*sim_b, entry_b.confidence);
    score_b.partial_cmp(&score_a)...
});

// New: add provenance boost to initial sort
results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
    let score_a = rerank_score(*sim_a, entry_a.confidence)
        + if entry_a.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
    let score_b = rerank_score(*sim_b, entry_b.confidence)
        + if entry_b.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
    score_b.partial_cmp(&score_a)...
});
```

### 2b. Co-Access Re-Sort (step 9c, inside `if !boost_map.is_empty()`)

```pseudo
// Current:
let final_a = base_a + boost_a;
let final_b = base_b + boost_b;

// New: add provenance boost
let prov_a = if entry_a.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
let prov_b = if entry_b.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
let final_a = base_a + boost_a + prov_a;
let final_b = base_b + boost_b + prov_b;
```

## 3. Application Site 2: uds_listener.rs (ContextSearch hook)

### 3a. Initial Sort (step 6)

Same pattern as tools.rs 2a:
```pseudo
results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
    let score_a = rerank_score(*sim_a, entry_a.confidence)
        + if entry_a.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
    let score_b = rerank_score(*sim_b, entry_b.confidence)
        + if entry_b.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
    score_b.partial_cmp(&score_a)...
});
```

### 3b. Co-Access Re-Sort (step 7, inside `if !boost_map.is_empty()`)

Same pattern as tools.rs 2b:
```pseudo
let prov_a = if entry_a.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
let prov_b = if entry_b.category == "lesson-learned" { PROVENANCE_BOOST } else { 0.0 };
let final_a = base_a + boost_a + prov_a;
let final_b = base_b + boost_b + prov_b;
```

## 4. Import

Both `tools.rs` and `uds_listener.rs` MUST import `PROVENANCE_BOOST` from
`unimatrix_engine::confidence::PROVENANCE_BOOST`. No magic numbers.

```pseudo
// tools.rs
use unimatrix_engine::confidence::PROVENANCE_BOOST;
// OR access as crate::confidence::PROVENANCE_BOOST if re-exported

// uds_listener.rs
use unimatrix_engine::confidence::PROVENANCE_BOOST;
```

## 5. Invariant Preservation

- `PROVENANCE_BOOST` is query-time only
- Never written to `EntryRecord.confidence`
- `W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92` unchanged
- Co-access affinity max (0.08) unchanged
- `PROVENANCE_BOOST = 0.02` < co-access max (0.03), acts as tiebreaker
