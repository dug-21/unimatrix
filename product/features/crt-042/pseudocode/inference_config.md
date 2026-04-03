# crt-042: InferenceConfig Additions — Pseudocode

## Purpose

Add three new operator-facing fields to `InferenceConfig` in
`crates/unimatrix-server/src/infra/config.rs`. All four coordinated sites must be updated
atomically in a single commit (entry #3817). All existing `InferenceConfig {}` struct literals
in tests must either include the three new fields or use `..Default::default()` (entries #2730,
#4044).

---

## Four Coordinated Sites

### Site 1: Struct Body

After the existing `max_s8_pairs_per_batch` field (~line 610), before the closing `}` of the
`InferenceConfig` struct, add:

```rust
    // -----------------------------------------------------------------------
    // Graph expand pool-widening fields (crt-042)
    // -----------------------------------------------------------------------

    /// Enable Phase 0 graph_expand candidate pool widening in the search pipeline.
    ///
    /// When true, Phase 0 runs BFS over TypedRelationGraph from HNSW seeds before
    /// PPR personalization vector construction. Expanded entries receive true cosine
    /// similarity scores and participate in PPR scoring.
    ///
    /// Default: false — gated behind A/B eval before default enablement (ADR-005, NFR-01).
    /// Remains false until MRR >= 0.2856 and P@5 > 0.1115 are confirmed, and P95 latency
    /// addition <= 50ms over pre-crt-042 baseline is measured.
    ///
    /// Validation: unconditional (ADR-004) — expansion_depth and max_expansion_candidates
    /// are always validated regardless of this flag value.
    #[serde(default = "default_ppr_expander_enabled")]
    pub ppr_expander_enabled: bool,

    /// BFS hop depth from seeds during Phase 0 graph expansion.
    ///
    /// Depth 1: only direct graph neighbors of seeds are reachable.
    /// Depth 2: neighbors of neighbors are also reachable.
    /// Higher depth increases candidate count and latency.
    ///
    /// Default: 2. Valid range: [1, 10] inclusive.
    #[serde(default = "default_expansion_depth")]
    pub expansion_depth: usize,

    /// Maximum number of entries Phase 0 may add to the candidate pool per query.
    ///
    /// BFS stops when this count is reached, processing frontier in sorted node-ID order.
    /// Combined ceiling with Phase 5: max_expansion_candidates (200) + ppr_max_expand (50)
    /// + HNSW k=20 = 270 maximum candidates before PPR scoring (SR-04, NFR-08).
    ///
    /// Default: 200. Valid range: [1, 1000] inclusive.
    #[serde(default = "default_max_expansion_candidates")]
    pub max_expansion_candidates: usize,
```

### Site 2: `impl Default for InferenceConfig`

After the existing `// crt-041: graph enrichment tick fields` block (~line 669), before the
closing `}` of the `InferenceConfig { ... }` literal:

```rust
            // crt-042: graph expand pool-widening fields
            ppr_expander_enabled: default_ppr_expander_enabled(),
            expansion_depth: default_expansion_depth(),
            max_expansion_candidates: default_max_expansion_candidates(),
```

### Site 3: Serde Default Functions

Add after the existing `fn default_ppr_max_expand()` function (~line 775):

```rust
fn default_ppr_expander_enabled() -> bool {
    false
}

fn default_expansion_depth() -> usize {
    2
}

fn default_max_expansion_candidates() -> usize {
    200
}
```

Pattern constraint (entry #3817): `default_ppr_expander_enabled()` must return `false`
(matching `Default::default()`). `default_expansion_depth()` must return `2`. 
`default_max_expansion_candidates()` must return `200`. All three must match their
corresponding `impl Default` values atomically — any divergence causes the serde-deserialized
default to differ from the programmatic default, producing a silent bug when omitting the
field from a TOML config file.

### Site 4: `InferenceConfig::validate()`

Add after the existing `ppr_max_expand` range check block (~line 1163), before the
`heal_pass_batch_size` check:

```rust
        // -- crt-042: expansion_depth range check [1, 10] inclusive --
        // Unconditional: validated regardless of ppr_expander_enabled (ADR-004).
        // Prevents NLI trap recurrence: invalid config caught at server start, not at flag-flip.
        if self.expansion_depth < 1 || self.expansion_depth > 10 {
            return Err(ConfigError::NliFieldOutOfRange {
                path: path.to_path_buf(),
                field: "expansion_depth",
                value: self.expansion_depth.to_string(),
                reason: "must be in range [1, 10] inclusive",
            });
        }

        // -- crt-042: max_expansion_candidates range check [1, 1000] inclusive --
        // Unconditional: validated regardless of ppr_expander_enabled (ADR-004).
        if self.max_expansion_candidates < 1 || self.max_expansion_candidates > 1000 {
            return Err(ConfigError::NliFieldOutOfRange {
                path: path.to_path_buf(),
                field: "max_expansion_candidates",
                value: self.max_expansion_candidates.to_string(),
                reason: "must be in range [1, 1000] inclusive",
            });
        }
```

Note on error variant: `ConfigError::NliFieldOutOfRange` is the existing error variant used
for all range checks in `validate()`. The field name parameters are plain `&'static str`
values. The `reason` string is human-readable and informational only.

---

## Merge Function Addition (merge_configs)

The three-level merge function at ~line 2545 must be extended. After the `max_s8_pairs_per_batch`
merge block, before the closing `},` of the inference section:

```rust
            // crt-042: graph expand pool-widening fields
            ppr_expander_enabled: if project.inference.ppr_expander_enabled
                != default.inference.ppr_expander_enabled
            {
                project.inference.ppr_expander_enabled
            } else {
                global.inference.ppr_expander_enabled
            },
            expansion_depth: if project.inference.expansion_depth
                != default.inference.expansion_depth
            {
                project.inference.expansion_depth
            } else {
                global.inference.expansion_depth
            },
            max_expansion_candidates: if project.inference.max_expansion_candidates
                != default.inference.max_expansion_candidates
            {
                project.inference.max_expansion_candidates
            } else {
                global.inference.max_expansion_candidates
            },
```

Merge semantics follow the existing `bool` and `usize` field pattern: project wins if
non-default, otherwise global. For bool: `!=` comparison. For usize: `!=` comparison.

---

## Hidden Test Sites (R-08, entry #4044)

After adding the three fields, grep the test suite for all `InferenceConfig {` struct literal
constructions. Each must either:
1. Include all three new fields explicitly, OR
2. Use `..Default::default()` spread (preferred for future-proofing)

Sites expected from historical pattern (grep `InferenceConfig {` in config.rs tests):
- `assert_validate_fails_with_field` helper (~line 4399): uses `..InferenceConfig::default()`
  spread — safe.
- Numerous `InferenceConfig { field_name, ..InferenceConfig::default() }` constructions
  in test body (~lines 4194–4660) — safe (use spread).
- Any literal construction without spread: must be updated.

---

## Error Handling

`validate()` returns `Result<(), ConfigError>`. The new range checks return
`Err(ConfigError::NliFieldOutOfRange { ... })` immediately on failure. Validation is called
once at server startup (`validate_config` in config.rs). A failed validation prevents the
server from starting.

Failure modes to test:
- `expansion_depth = 0` → validate() error (AC-18)
- `expansion_depth = 11` → validate() error (AC-19)
- `max_expansion_candidates = 0` → validate() error (AC-20)
- `max_expansion_candidates = 1001` → validate() error (AC-21)

All four must fail regardless of `ppr_expander_enabled` value (ADR-004, R-14).

---

## Key Test Scenarios

**AC-17 — TOML omission produces defaults.**
`toml::from_str::<UnimatrixConfig>("[inference]\n")` or `toml::from_str::<InferenceConfig>("")`
produces `ppr_expander_enabled = false`, `expansion_depth = 2`, `max_expansion_candidates = 200`.
Tests both the serde default functions and the field-omission behavior.

**AC-18 — depth=0 fails unconditionally.**
`InferenceConfig { expansion_depth: 0, ppr_expander_enabled: false, ..Default::default() }`
→ `validate()` returns Err. Fails even when flag is false.

**AC-19 — depth=11 fails unconditionally.**
`InferenceConfig { expansion_depth: 11, ppr_expander_enabled: false, ..Default::default() }`
→ `validate()` returns Err.

**AC-20 — max=0 fails unconditionally.**
`InferenceConfig { max_expansion_candidates: 0, ppr_expander_enabled: false, ..Default::default() }`
→ `validate()` returns Err.

**AC-21 — max=1001 fails unconditionally.**
`InferenceConfig { max_expansion_candidates: 1001, ppr_expander_enabled: false, ..Default::default() }`
→ `validate()` returns Err.

**R-08 — default value atomicity.**
Assert `InferenceConfig::default().ppr_expander_enabled == false`.
Assert `InferenceConfig::default().expansion_depth == 2`.
Assert `InferenceConfig::default().max_expansion_candidates == 200`.
Assert serde default fns return identical values.

**R-08 — merge propagation.**
Construct a project config with `ppr_expander_enabled = true`, `expansion_depth = 3`,
`max_expansion_candidates = 100`. Merge with global default. Assert merged values are
`true`, `3`, `100` (project wins — all non-default).

**Config toml roundtrip (pattern #3928).**
`toml::from_str::<UnimatrixConfig>("[inference]\nppr_expander_enabled = true\nexpansion_depth = 3\nmax_expansion_candidates = 150\n")`
Assert parsed values are `true`, `3`, `150`.
