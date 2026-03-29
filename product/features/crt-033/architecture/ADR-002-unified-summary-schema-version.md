## ADR-002: Unified SUMMARY_SCHEMA_VERSION in cycle_review_index.rs

### Context

Stored `summary_json` encodes two kinds of staleness:

1. **Serialization format staleness**: a field was added/removed/renamed on
   `RetrospectiveReport` or any nested type. A stored record may lack the field entirely or
   carry a renamed field that no longer deserializes correctly.

2. **Detection rule staleness**: hotspot detection rules in `unimatrix-observe` changed
   (new rule added, threshold changed, rule removed). A stored record's `hotspots` field
   was computed under different logic than what is currently deployed.

Three options were considered for tracking version:

**Option A: two separate constants — `SUMMARY_JSON_VERSION` and `DETECTION_RULES_VERSION`**
More precise (callers can distinguish rule-only staleness from structural staleness). But
adds cross-crate coupling: `DETECTION_RULES_VERSION` would need to live in `unimatrix-observe`
and be imported by `unimatrix-store`. This couples the store crate to the observe crate's
versioning. Additionally, the stored record has only one `schema_version` column — a two-
version scheme would require a second column or encoding both into one integer, adding
complexity for marginal benefit (the advisory is informational only; `force=true` is the
user-facing action regardless of cause).

**Option B: SUMMARY_SCHEMA_VERSION in unimatrix-observe**
Removes cross-crate coupling for the observe side but creates it in the store side (store
would import from observe). Worse: `unimatrix-store` must not depend on `unimatrix-observe`
(observe depends on store, not the reverse). This creates a circular dependency.

**Option C: SUMMARY_SCHEMA_VERSION in cycle_review_index.rs only (unified)**
Single integer, defined in the store crate alongside the record type. No cross-crate
dependency. Bump policy: increment whenever either detection rules OR serialization format
changes in a way that would produce a meaningfully different stored record. The advisory
message ("computed with schema_version N, current is M — use force=true to recompute") is
correct for both causes.

SR-03 risk accepted: a detection-rule change triggers an advisory even if the JSON structure
is stable. This is intentional — the stored `hotspots` field is stale regardless of whether
the structural format changed. Callers should not rely on stored hotspot findings being
current; they should use `force=true` when freshness matters.

### Decision

Define `pub const SUMMARY_SCHEMA_VERSION: u32 = 1` in
`crates/unimatrix-store/src/cycle_review_index.rs`.

Bump policy:
- Bump when any field is added, removed, or renamed on `RetrospectiveReport` or any nested
  type in a way that affects JSON round-trip fidelity.
- Bump when any hotspot detection rule is added, removed, or has its scoring logic changed.
- Do NOT bump for threshold-only changes that leave existing stored results technically
  correct.

The advisory message template is fixed: "computed with schema_version {stored}, current is
{SUMMARY_SCHEMA_VERSION} — use force=true to recompute." This message is appended to the
response when versions differ; the stored record is always returned (never silently
recomputed).

No `unimatrix-observe` import in `unimatrix-store`. No second column in
`cycle_review_index`. Unified single integer.

### Consequences

**Easier**:
- No circular crate dependency.
- Single bump point: increment one constant when either detection logic or JSON shape changes.
- Advisory message covers both staleness causes correctly.

**Harder**:
- A detection-rule change generates an advisory even for callers who don't care about hotspot
  freshness (e.g., they only read `metrics`). This is a documentation trade-off, not a
  correctness issue. The stored `metrics` field is unaffected by rule changes; the advisory
  is conservative.
- Bump discipline requires awareness of two change domains (observe rules + store serde). A
  comment in `cycle_review_index.rs` must document both bump triggers.
