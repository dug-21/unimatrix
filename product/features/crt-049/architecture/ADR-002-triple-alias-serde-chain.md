## ADR-002: Triple-Alias Serde Chain on `search_exposure_count`

### Context

`FeatureKnowledgeReuse.delivery_count` currently has one alias: `#[serde(alias = "tier1_reuse_count")]`
(historical from col-020b). Renaming `delivery_count` → `search_exposure_count` introduces
a second alias requirement. All stored `cycle_review_index.summary_json` rows serialized
before crt-049 will contain the key `"delivery_count"`. Any row serialized before col-020b
will contain `"tier1_reuse_count"`.

SR-02 from the risk assessment flags that Rust's serde alias resolution with multiple
`alias` attributes on a single field is load-bearing and non-obvious. Specifically:
- Rust serde resolves aliases left-to-right; duplicates in the input JSON use the last seen.
- `#[serde(rename = "search_exposure_count", alias = "delivery_count", alias = "tier1_reuse_count")]`
  is the correct form — serde applies `rename` for serialization output, `alias` for
  deserialization compatibility only.
- Serde does NOT support multiple `alias` in a single attribute expression on all versions;
  each alias requires its own `#[serde(alias = "...")]` attribute line.

Three options:
- Option A: Single combined attribute `#[serde(rename = "search_exposure_count", alias = "delivery_count", alias = "tier1_reuse_count")]`.
  Risk: depends on serde version supporting multiple alias in one macro invocation.
- Option B: Stacked attribute lines:
  ```rust
  #[serde(alias = "delivery_count")]
  #[serde(alias = "tier1_reuse_count")]
  pub search_exposure_count: u64,
  ```
  Guaranteed to work across all serde versions; each `alias` is processed independently.
- Option C: Add a custom deserialize impl. Rejected: high complexity for no gain.

Option B is the mandated form. Lesson #885 (referenced in SCOPE-RISK-ASSESSMENT SR-02):
serde-heavy types cause gate failures when round-trip tests are omitted.

### Decision

The renamed field in `FeatureKnowledgeReuse` (`types.rs`) MUST use stacked attributes:

```rust
/// Total unique entry IDs appearing in search results across sessions.
/// Formerly `delivery_count`; renamed in crt-049 for semantic clarity.
#[serde(alias = "delivery_count")]
#[serde(alias = "tier1_reuse_count")]
pub search_exposure_count: u64,
```

The implementation spec MUST mandate three serde round-trip test cases, each non-negotiable
(AC-10, AC-12):
1. JSON with key `"search_exposure_count"` deserializes correctly.
2. JSON with key `"delivery_count"` deserializes correctly (stored pre-crt-049 rows).
3. JSON with key `"tier1_reuse_count"` deserializes correctly (stored pre-col-020b rows).

All three must also verify that serialization output uses `"search_exposure_count"` as
the canonical key (i.e., the round-trip test: serialize → deserialize → field value matches).

### Consequences

Easier:
- Option B is portable across serde versions — no dependency on multi-alias in one attribute.
- The three test cases structurally prevent regression when serde is updated.

Harder:
- Test fixtures in `types.rs` and `retrospective.rs` that construct `FeatureKnowledgeReuse`
  with `delivery_count: N` must be updated to `search_exposure_count: N`. These are
  compile-time changes, not serde issues — the struct field name changes.
- Existing JSON strings in tests that contain `"delivery_count"` as the serialized key
  will now produce `"search_exposure_count"` in serialized output, breaking any golden-output
  string comparisons that check for the literal key `"delivery_count"`.
