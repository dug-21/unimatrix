## ADR-005: InferenceConfig Dual-Maintenance Guard for Five New Fields

### Context

`InferenceConfig` in `crates/unimatrix-server/src/infra/config.rs` uses two
independent mechanisms to encode default values for each field:

1. A private `default_*()` function annotated with `#[serde(default = "fn_name")]`
   on the field — used when the field is absent from `config.toml` at startup
   (TOML deserialization path).

2. The `impl Default for InferenceConfig` struct literal — used when code calls
   `InferenceConfig::default()` directly (programmatic path, tests, merge_configs).

These two sites encode the same value independently. Unimatrix pattern #3817 documents
the trap: "Changing only one leaves an inconsistent default — serde path and programmatic
path return different values." This was a concrete bug in crt-032 (w_coac field:
serde fn used 0.10, Default impl used 0.0 — diverged silently).

SR-07 from the SCOPE-RISK-ASSESSMENT identifies this as High severity / High likelihood
specifically because both crt-040 and crt-041 add `InferenceConfig` fields in the
same delivery period, and a crt-040 merge that updates only one site would corrupt
the defaults seen by crt-041 tests.

crt-041 adds five new fields. The procedure for adding InferenceConfig fields (#3769)
requires five update locations for each field:
1. Field declaration in struct with `#[serde(default = "fn_name")]`
2. Private `default_*()` function (serde path)
3. `validate()` range check — mandatory if field has a documented range
4. `impl Default` struct literal entry (programmatic path)
5. `merge_configs()` block entry

**The dual-maintenance invariant:** For each field, the value in the `default_*()` function
MUST equal the value in the `impl Default` struct literal. These are the two mutation
sites that must change atomically.

**Five new fields with their dual-site values:**

| Field | Type | `default_*()` value | `impl Default` value | Range | validate() |
|-------|------|---------------------|---------------------|-------|------------|
| `s2_vocabulary` | `Vec<String>` | `vec![]` (empty) | `vec![]` | n/a (empty is valid) | none needed |
| `max_s1_edges_per_tick` | `usize` | `200` | `200` | [1, 10000] | required |
| `max_s2_edges_per_tick` | `usize` | `200` | `200` | [1, 10000] | required |
| `s8_batch_interval_ticks` | `u32` | `10` | `10` | [1, 1000] | required |
| `max_s8_pairs_per_batch` | `usize` | `500` | `500` | [1, 10000] | required |

**s2_vocabulary default rationale:** Empty vec (not the 9-term ASS-038 list). SCOPE.md
§Design Decision 3 explicitly resolves this: the 9-term software engineering list
re-couples the product to a specific domain, violating the domain-agnostic product
vision (W0-3). S2 is a no-op out of the box. The 9-term list is documented in the
field's doc-comment as the recommended software-engineering starting point.

**validate() checks (four range-bounded fields):**

```rust
// max_s1_edges_per_tick: [1, 10000]
if self.max_s1_edges_per_tick < 1 || self.max_s1_edges_per_tick > 10000 {
    return Err(ConfigError::NliFieldOutOfRange("max_s1_edges_per_tick".into()));
}
// max_s2_edges_per_tick: [1, 10000]
if self.max_s2_edges_per_tick < 1 || self.max_s2_edges_per_tick > 10000 {
    return Err(ConfigError::NliFieldOutOfRange("max_s2_edges_per_tick".into()));
}
// s8_batch_interval_ticks: [1, 1000]
if self.s8_batch_interval_ticks < 1 || self.s8_batch_interval_ticks > 1000 {
    return Err(ConfigError::NliFieldOutOfRange("s8_batch_interval_ticks".into()));
}
// max_s8_pairs_per_batch: [1, 10000]
if self.max_s8_pairs_per_batch < 1 || self.max_s8_pairs_per_batch > 10000 {
    return Err(ConfigError::NliFieldOutOfRange("max_s8_pairs_per_batch".into()));
}
```

All four use lower bound 1 (not 0) because these fields are used as SQL LIMIT
and modulo divisors. A value of 0 would produce `LIMIT 0` (silent no-op) or
`% 0` (integer division by zero panic). The validate() check is not optional —
lesson #3766 documents the bugfix-444 case where a missing validate() for a
LIMIT field silently disabled a feature.

**Pre-delivery verification step (SR-07):**

Before opening a crt-041 PR, the delivery agent must run:
```
cargo test -p unimatrix-server -- test_inference_config_defaults_match_serde 2>&1 | tail -5
```

This test (to be added in crt-041 delivery) asserts that for every new field,
`InferenceConfig::default().field == toml::from_str("").unwrap().field`. It catches
any divergence between the two sites. The test is in `config.rs::tests` alongside
the existing `test_inference_config_defaults_pass_validation` test.

### Decision

Five new fields added to `InferenceConfig` in `config.rs` following the procedure
in Unimatrix entry #3769. Both mutation sites (serde default function AND impl Default
struct literal) use identical values as enumerated in the table above.

A test named `test_inference_config_s1_s2_s8_defaults_match_serde` is added to
`config.rs::tests` asserting that the five new fields produce identical values
from `InferenceConfig::default()` and `toml::from_str("")` (empty TOML → all fields
use serde defaults).

The `crt-041` comment group is added to both the struct field block and the
`impl Default` struct literal, parallel to `// crt-034: co_access promotion tick fields`.

### Consequences

Easier: The dual-site test makes divergence a compile-time-catchable test failure,
not a silent runtime behavioral difference. Future features adding InferenceConfig
fields can use this test as a template.

Harder: One additional test to write and maintain. The `s2_vocabulary` field requires
a non-trivial default (empty vec) vs the 9-term list in the doc-comment — operators
must explicitly configure S2 to use it, which could be surprising. The doc-comment
must clearly document this is intentional and provide the example list.
