# Component 1: FeatureKnowledgeReuse — `unimatrix-observe/src/types.rs`

## Purpose

Define the shared report type that carries knowledge reuse metrics, consumed by
`compute_knowledge_reuse` (server crate) and rendered by `render_knowledge_reuse`.
This component adds two new fields, renames one field with backward-compat serde aliases,
and documents the semantic change to `total_served`.

---

## Scope of Changes

The implementation agent modifies the `FeatureKnowledgeReuse` struct definition only.
All computation is in `knowledge_reuse.rs`; all rendering is in `retrospective.rs`.
This file change is purely a type definition change.

---

## Modified Struct Definition

File: `crates/unimatrix-observe/src/types.rs`

### Field: `search_exposure_count` (renamed from `delivery_count`)

```
// Old:
#[serde(alias = "tier1_reuse_count")]
pub delivery_count: u64,

// New (crt-049):
/// Count of distinct entry IDs returned in query result sets during the cycle.
/// Renamed from `delivery_count` (crt-049). Does NOT imply the agent consumed the entry.
/// Serde aliases retain round-trip compatibility with pre-crt-049 stored rows
/// ("delivery_count") and pre-col-020b stored rows ("tier1_reuse_count").
/// BOTH aliases are required — dropping either silently produces zero on re-review.
#[serde(alias = "delivery_count")]
#[serde(alias = "tier1_reuse_count")]
pub search_exposure_count: u64,
```

Attribute order: each alias on its own `#[serde(...)]` line (ADR-002).
Canonical serialization key: `"search_exposure_count"` (Rust field name, no `rename`).

### Field: `explicit_read_count` (new)

```
/// Count of distinct entry IDs explicitly retrieved by agents via context_get
/// or single-ID context_lookup. Unambiguous consumption signal.
/// Defaults to 0 when absent in stored JSON (pre-crt-049 rows).
#[serde(default)]
pub explicit_read_count: u64,
```

### Field: `explicit_read_by_category` (new)

```
/// Per-category tally of explicit read IDs, joined via batch_entry_meta_lookup.
/// Cycle-level breakdown (no phase dimension). Used as human-facing reporting
/// and correctness signal for ASS-040 Group 10.
/// NOT the primary Group 10 training input — Group 10 requires phase-stratified
/// (phase, category) aggregates from observations directly (C-08, out of scope).
/// Category strings match entries.category canonical values.
/// Defaults to empty map when absent in stored JSON (pre-crt-049 rows).
#[serde(default)]
pub explicit_read_by_category: HashMap<String, u64>,
```

### Field: `total_served` (semantic change, definition unchanged)

```
/// Redefined in crt-049: count of distinct entry IDs in the deduplicated union
/// of explicit_read_ids and injection_ids. Search exposures excluded.
/// Computed in compute_knowledge_reuse; populated by caller via the returned struct.
/// Previously was an alias for delivery_count; now has independent semantics.
#[serde(default)]
pub total_served: u64,
```

### Unchanged Fields

`cross_session_count`, `by_category`, `category_gaps`, `total_stored`,
`cross_feature_reuse`, `intra_cycle_reuse`, `top_cross_feature_entries` — no changes.

---

## Field Ordering in Struct

Recommended field order to minimize cognitive dissonance with existing code:

```
pub search_exposure_count: u64,      // renamed from delivery_count
pub explicit_read_count: u64,        // new
pub explicit_read_by_category: ...,  // new
pub cross_session_count: u64,        // unchanged
pub by_category: HashMap<...>,       // unchanged
pub category_gaps: Vec<String>,      // unchanged
pub total_served: u64,               // semantic change
pub total_stored: u64,               // unchanged
pub cross_feature_reuse: u64,        // unchanged
pub intra_cycle_reuse: u64,          // unchanged
pub top_cross_feature_entries: ...,  // unchanged
```

---

## Test Scenarios for This Component

All tests live in the `#[cfg(test)]` module at the bottom of `types.rs`.
Extend the existing test module — do not create a separate file.

### AC-02 GATE — Triple-alias serde round-trip (five sub-cases, all required)

Test name: `test_search_exposure_count_alias_round_trip`

```
Sub-case (a): canonical key
    json = {"search_exposure_count": 42, ...required other fields...}
    let r: FeatureKnowledgeReuse = from_str(json)
    assert r.search_exposure_count == 42

Sub-case (b): delivery_count alias (pre-crt-049 stored rows)
    json = {"delivery_count": 42, ...}
    let r: FeatureKnowledgeReuse = from_str(json)
    assert r.search_exposure_count == 42

Sub-case (c): tier1_reuse_count alias (pre-col-020b stored rows)
    json = {"tier1_reuse_count": 42, ...}
    let r: FeatureKnowledgeReuse = from_str(json)
    assert r.search_exposure_count == 42

Sub-case (d): canonical serialization key
    let r = FeatureKnowledgeReuse { search_exposure_count: 99, ... }
    let json = to_string(r)
    assert json.contains("\"search_exposure_count\"")
    assert !json.contains("\"delivery_count\"")

Sub-case (e): full round-trip
    let original = FeatureKnowledgeReuse { search_exposure_count: 7, ... }
    let json = to_string(original)
    let back: FeatureKnowledgeReuse = from_str(json)
    assert back.search_exposure_count == 7
```

Failure mode if missing: stored rows with `"delivery_count"` key silently deserialize
`search_exposure_count` as `0` on re-review — no error, no diagnostic.

### AC-01 — explicit_read_count default

```
Test name: test_explicit_read_count_default
    json = {"search_exposure_count": 0, "by_category": {}, "category_gaps": [],
            "total_stored": 0, "cross_feature_reuse": 0, "intra_cycle_reuse": 0,
            "top_cross_feature_entries": []}
    let r: FeatureKnowledgeReuse = from_str(json)
    assert r.explicit_read_count == 0   // serde(default) applied
    assert r.explicit_read_by_category == {}  // serde(default) applied
```

### AC-13 GATE — explicit_read_by_category field contract

```
Test name: test_explicit_read_by_category_serde
    let r = FeatureKnowledgeReuse {
        explicit_read_by_category: {"decision": 3, "pattern": 1},
        ...
    }
    let json = to_string(r)
    let back: FeatureKnowledgeReuse = from_str(json)
    assert back.explicit_read_by_category.get("decision") == Some(3)
    assert back.explicit_read_by_category.get("pattern") == Some(1)
```

### Fixture update requirement (R-13)

Every existing test in `types.rs` and `retrospective.rs` that constructs
`FeatureKnowledgeReuse` using the Rust field name `delivery_count` must be updated to
`search_exposure_count`. These are compile-time errors — they will not compile after the
rename.

Every existing test that asserts a serialized JSON key `"delivery_count"` in golden output
must be updated to assert `"search_exposure_count"` instead. These are NOT compile-time
errors — they silently pass while asserting the wrong key.

---

## Error Handling

This component has no runtime error paths. All changes are struct definition and serde
attribute metadata. Serde deserialization errors on stored JSON (malformed rows) are
handled by callers, not by this struct.

---

## Integration Surface

| Name | Type | Consumer |
|------|------|----------|
| `FeatureKnowledgeReuse::search_exposure_count` | `u64` | `compute_knowledge_reuse`, `render_knowledge_reuse` |
| `FeatureKnowledgeReuse::explicit_read_count` | `u64` | `compute_knowledge_reuse`, `render_knowledge_reuse` |
| `FeatureKnowledgeReuse::explicit_read_by_category` | `HashMap<String, u64>` | `compute_knowledge_reuse`, `render_knowledge_reuse`, ASS-040 Group 10 |
| `FeatureKnowledgeReuse::total_served` | `u64` | `render_knowledge_reuse` |
