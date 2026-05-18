# Component 3: RelationType Enum Extension

## Purpose

Add 10 new variants to the `RelationType` enum in `crates/unimatrix-engine/src/graph.rs`.
Each variant requires exactly 4 coordinated update sites. Missing any one site causes either
a compile error (sites 1, 2) or a silent row-drop at Pass 2b (site 3 — from_str).

This component is a prerequisite for all other components: `edge_write.rs` calls
`RelationType::from_str()` and `as_str()`; PPR/expand use `RelationType::RelatedTo`.

## File

`crates/unimatrix-engine/src/graph.rs` (modified)

## 10 New Variants — Complete Specification

| Variant | as_str() output | Storage direction | PPR/BFS positive |
|---------|----------------|-------------------|-----------------|
| `Advances` | `"Advances"` | A→B (directed) | NO — write-only, Phase 2 |
| `Cites` | `"Cites"` | A→B | NO — write-only |
| `Asserts` | `"Asserts"` | A→B | NO — write-only |
| `Mentions` | `"Mentions"` | A→B | NO — write-only |
| `Refutes` | `"Refutes"` | A→B | NO — write-only |
| `Tests` | `"Tests"` | A→B | NO — write-only |
| `DerivedFrom` | `"DerivedFrom"` | A→B | NO — write-only |
| `Motivates` | `"Motivates"` | A→B (directed) | NO — write-only, Phase 2 |
| `About` | `"About"` | A→B | NO — write-only |
| `RelatedTo` | `"RelatedTo"` | A→B (symmetric at architect's discretion) | YES |

## Site 1: Enum Body Addition

```
// In: crates/unimatrix-engine/src/graph.rs
// Location: RelationType enum body (after existing 6 variants)

pub enum RelationType {
    // ── Existing 6 (UNCHANGED) ───────────────────────────
    Supersedes,
    Contradicts,
    Supports,
    CoAccess,
    Prerequisite,
    Informs,
    // ── 10 New Variants (vnc-015) ────────────────────────
    // SDLC goal-tracing
    Advances,       // source advances or contributes toward target goal/objective
    Motivates,      // source is motivation/rationale behind target decision

    // Research domain
    Cites,          // source cites/references target as primary source
    Asserts,        // source makes or contains target claim
    Mentions,       // source mentions target entity
    Refutes,        // source provides evidence contradicting/falsifying target
    Tests,          // source tests or experimentally evaluates target thesis/claim
    DerivedFrom,    // source is derived from or originated in target
    About,          // source concerns or governs target entity/concept

    // General fallback (the only new PPR-positive variant)
    RelatedTo,      // weak semantic relatedness; no more specific type available
}
```

## Site 2: as_str() Match Arms

```
// In: impl RelationType { fn as_str(&self) -> &'static str { ... } }
// Append to existing exhaustive match:

RelationType::Advances    => "Advances",
RelationType::Cites       => "Cites",
RelationType::Asserts     => "Asserts",
RelationType::Mentions    => "Mentions",
RelationType::Refutes     => "Refutes",
RelationType::Tests       => "Tests",
RelationType::DerivedFrom => "DerivedFrom",
RelationType::Motivates   => "Motivates",
RelationType::About       => "About",
RelationType::RelatedTo   => "RelatedTo",
```

The match must remain exhaustive. If the existing as_str() uses a wildcard arm (`_ => ...`),
the compiler will not catch a missing arm — verify by inspection that each variant is listed.

## Site 3: from_str() Match Arms

```
// In: impl RelationType { fn from_str(s: &str) -> Option<Self> { ... } }
// OR: impl std::str::FromStr for RelationType
// Append to existing arms BEFORE any wildcard/default arm:

"Advances"    => Some(RelationType::Advances),
"Cites"       => Some(RelationType::Cites),
"Asserts"     => Some(RelationType::Asserts),
"Mentions"    => Some(RelationType::Mentions),
"Refutes"     => Some(RelationType::Refutes),
"Tests"       => Some(RelationType::Tests),
"DerivedFrom" => Some(RelationType::DerivedFrom),
"Motivates"   => Some(RelationType::Motivates),
"About"       => Some(RelationType::About),
"RelatedTo"   => Some(RelationType::RelatedTo),
// Wildcard arm MUST come AFTER all named arms:
_ => None,
```

CRITICAL: The wildcard arm (`_ => None`) must remain as the last arm. A variant added after
the wildcard will compile but never match — all calls return None and edges are silently
dropped at Pass 2b. Order matters here even though the compiler won't warn about it.

## Site 4: PPR Positive Type Inclusion (RelatedTo ONLY)

Site 4 is defined in `ppr-expand.md` (Component 4). This component only defines the enum.

The 10×4 compliance matrix (ADR-007):

| Variant | Site 1 (enum body) | Site 2 (as_str) | Site 3 (from_str) | Site 4 (PPR/BFS) |
|---------|:-:|:-:|:-:|:-:|
| Advances | required | required | required | INTENTIONALLY ABSENT (Phase 2) |
| Cites | required | required | required | intentionally absent |
| Asserts | required | required | required | intentionally absent |
| Mentions | required | required | required | intentionally absent |
| Refutes | required | required | required | intentionally absent |
| Tests | required | required | required | intentionally absent |
| DerivedFrom | required | required | required | intentionally absent |
| Motivates | required | required | required | INTENTIONALLY ABSENT (Phase 2) |
| About | required | required | required | intentionally absent |
| RelatedTo | required | required | required | REQUIRED |

## Pass 2b Survival — build_typed_relation_graph R-10 Guard

`build_typed_relation_graph` in graph.rs iterates GRAPH_EDGES rows and calls `from_str()` on
each `relation_type` string. If `from_str()` returns `None`, the edge is logged as a warning
and silently dropped — it never appears in the `TypedRelationGraph`. This is the R-10 guard.

A missing `from_str()` arm means edges written with that variant type are invisible to PPR,
graph_expand, and any traversal — even though they exist in the DB. No error is emitted to the
write caller. The failure mode is silent data loss in traversal.

The implementation must verify all 10 arms parse before delivery (Gate-3a grep + integration
test per variant per ADR-007).

## Error Handling

The enum extension itself has no runtime error paths. Compile errors on missing as_str() arms
(exhaustive match) are the safety net for sites 1 and 2. Site 3 (from_str) has no compile-time
guard — only the integration test catches a missing arm.

## Key Test Scenarios

1. Round-trip test for all 10 variants: `RelationType::from_str(v.as_str()) == Some(v)` (AC-03)
2. Round-trip test for all 6 existing variants: unchanged (AC-04)
3. Per-variant Pass 2b survival test: insert GRAPH_EDGES row with variant string; call
   `build_typed_relation_graph`; assert edge count = 1 (not 0) (AC-14, R-01)
4. Unknown string: `RelationType::from_str("Unknown") == None`
5. Case-sensitivity: `RelationType::from_str("advances") == None` (lowercase must not match)
6. Negative check: `Advances` and `Motivates` must NOT appear in PPR positive type sets (AC-17, R-11)
