## ADR-002: config.toml Field-by-Field Verified Defaults Table

### Context

SR-01 (High/High risk) identified that the current `config.toml` documents only the `[retention]` section and that two independent default sites exist in `config.rs`: the `#[serde(default = "fn")]` functions (governing TOML omission behavior) and the `Default` impl (governing programmatic construction). These must match. The risk assessment recommends the implementer produce a side-by-side verified table. This ADR establishes that table as an authoritative artifact and defines which fields are user-facing vs. internal.

The implementer must read `default_*` functions directly from `crates/unimatrix-server/src/infra/config.rs` and verify each value before writing any field into `config.toml`. This ADR records the verified values extracted during architecture design.

### Decision

**Verified defaults extracted from config.rs `default_*` functions and `Default` impls:**

#### [profile]
| TOML key | Rust field | Type | Default | Verified |
|---|---|---|---|---|
| `preset` | `ProfileConfig.preset` | enum | `"collaborative"` | Yes — `#[default]` on `Preset::Collaborative`, `#[serde(rename_all = "lowercase")]` |

Preset enum values (lowercase in TOML): `collaborative`, `authoritative`, `operational`, `empirical`, `custom`

#### [knowledge]
| TOML key | Rust field | Type | Default | Verified |
|---|---|---|---|---|
| `categories` | `KnowledgeConfig.categories` | `Vec<String>` | `["lesson-learned","decision","convention","pattern","procedure"]` (5 INITIAL_CATEGORIES) | Yes — `Default` impl reads `INITIAL_CATEGORIES` from `categories/mod.rs` |
| `boosted_categories` | `KnowledgeConfig.boosted_categories` | `Vec<String>` | `["lesson-learned"]` (serde default) | Yes — `default_boosted_categories()` returns `vec!["lesson-learned"]`; NOTE: programmatic `Default::default()` returns `vec![]` — TOML omission uses the serde default |
| `adaptive_categories` | `KnowledgeConfig.adaptive_categories` | `Vec<String>` | `["lesson-learned"]` (serde default) | Yes — `default_adaptive_categories()` returns `vec!["lesson-learned"]`; same two-site distinction as above |
| `freshness_half_life_hours` | `KnowledgeConfig.freshness_half_life_hours` | `Option<f64>` | absent (None = use preset's built-in value) | Yes — field is `Option<f64>`, no serde default fn; omitted means None |

#### [server]
| TOML key | Rust field | Type | Default | Verified |
|---|---|---|---|---|
| `instructions` | `ServerConfig.instructions` | `Option<String>` | absent (None = use compiled SERVER_INSTRUCTIONS) | Yes — `Option<String>` with `#[serde(default)]` on struct |

#### [agents]
| TOML key | Rust field | Type | Default | Verified |
|---|---|---|---|---|
| `default_trust` | `AgentsConfig.default_trust` | `String` | `"permissive"` | Yes — `Default` impl |
| `session_capabilities` | `AgentsConfig.session_capabilities` | `Vec<String>` | `["Read","Write","Search"]` | Yes — `Default` impl; case-sensitive: capital R/W/S |

#### [retention]
| TOML key | Rust field | Type | Default | Verified |
|---|---|---|---|---|
| `activity_detail_retention_cycles` | `RetentionConfig.activity_detail_retention_cycles` | `u32` | `50` | Yes — `default_activity_detail_retention_cycles()` |
| `audit_log_retention_days` | `RetentionConfig.audit_log_retention_days` | `u32` | `180` | Yes — `default_audit_log_retention_days()` |
| `max_cycles_per_tick` | `RetentionConfig.max_cycles_per_tick` | `u32` | `10` | Yes — `default_max_cycles_per_tick()` |

#### [observation] — DomainPackConfig (table-of-tables)
| TOML key | Rust field | Type | Required? | Verified |
|---|---|---|---|---|
| `source_domain` | `DomainPackConfig.source_domain` | `String` | Required | Yes — no `#[serde(default)]`; absent = parse error |
| `event_types` | `DomainPackConfig.event_types` | `Vec<String>` | Required | Yes — no `#[serde(default)]` |
| `categories` | `DomainPackConfig.categories` | `Vec<String>` | Required | Yes — no `#[serde(default)]` |
| `rule_file` | `DomainPackConfig.rule_file` | `Option<PathBuf>` | Optional | Yes — `#[serde(default)]` on field; absent = None |

Note: `source_domain`, `event_types`, and `categories` have no struct-level default — omitting them is a parse error at startup. The config comment block must label these as required.

#### [confidence] — custom preset only
| TOML key | Rust field | Type | Default | Verified |
|---|---|---|---|---|
| `weights.base` | `ConfidenceWeights.base` | `f64` | no default (required when preset=custom) | Yes — struct has no `Default` to prevent zero-init |
| `weights.usage` | `ConfidenceWeights.usage` | `f64` | no default | Yes |
| `weights.fresh` | `ConfidenceWeights.fresh` | `f64` | no default | Yes |
| `weights.help` | `ConfidenceWeights.help` | `f64` | no default | Yes |
| `weights.corr` | `ConfidenceWeights.corr` | `f64` | no default | Yes |
| `weights.trust` | `ConfidenceWeights.trust` | `f64` | no default | Yes |

Sum constraint: `base + usage + fresh + help + corr + trust` must equal `0.92 ± 1e-9`.

#### [inference] — operator-facing fields
| TOML key | Rust field | Type | Default | Verified |
|---|---|---|---|---|
| `rayon_pool_size` | `InferenceConfig.rayon_pool_size` | `usize` | `(num_cpus::get() / 2).max(4).min(8)` — dynamic | Yes — `Default` impl |
| `phase_freq_lookback_days` | `InferenceConfig.phase_freq_lookback_days` | `u32` | `30` | Yes — `default_phase_freq_lookback_days()` |
| `min_phase_session_pairs` | `InferenceConfig.min_phase_session_pairs` | `u32` | `5` | Yes — `default_min_phase_session_pairs()` |

#### [inference] — NLI block (operator-facing, opt-in, fully commented out in config.toml)
| TOML key | Rust field | Type | Default | Verified |
|---|---|---|---|---|
| `nli_enabled` | `InferenceConfig.nli_enabled` | `bool` | `false` | Yes — `default_nli_enabled()` |
| `nli_model_name` | `InferenceConfig.nli_model_name` | `Option<String>` | absent (None = `NliMiniLM2L6H768Q8`) | Yes — `#[serde(default)]` on field |
| `nli_model_path` | `InferenceConfig.nli_model_path` | `Option<PathBuf>` | absent | Yes |
| `nli_model_sha256` | `InferenceConfig.nli_model_sha256` | `Option<String>` | absent | Yes |
| `nli_top_k` | `InferenceConfig.nli_top_k` | `usize` | `20` | Yes — `default_nli_top_k()` |
| `nli_entailment_threshold` | `InferenceConfig.nli_entailment_threshold` | `f32` | `0.6` | Yes — `default_nli_entailment_threshold()` |
| `nli_contradiction_threshold` | `InferenceConfig.nli_contradiction_threshold` | `f32` | `0.6` | Yes — `default_nli_contradiction_threshold()` |

#### [inference] — internal tuning fields (present in "do not change" block or omitted)
These fields are confirmed internal tuning knobs. They must appear in a clearly-marked "Internal tuning — do not change unless directed by a support issue" block, or be omitted entirely from the default config. The implementer chooses based on what reduces operator confusion. Key values for completeness:

| TOML key | Default | Notes |
|---|---|---|
| `ppr_alpha` | `0.85` | PPR damping factor |
| `ppr_iterations` | `20` | Power iteration count |
| `ppr_inclusion_threshold` | `0.05` | PPR score floor for pool injection |
| `ppr_blend_weight` | `0.15` | PPR trust weight |
| `ppr_max_expand` | `50` | Max PPR-injected entries per query |
| `ppr_expander_enabled` | `false` | Graph expand pool widening (gated) |
| `w_sim` | `0.50` | Fusion: cosine similarity weight |
| `w_conf` | `0.35` | Fusion: confidence weight |
| `w_phase_histogram` | `0.02` | Fusion: session histogram affinity |
| `w_phase_explicit` | `0.05` | Fusion: explicit phase signal |
| `supports_cosine_threshold` | `0.65` | Cosine Supports detection threshold |

**config.toml section order (mandatory):**
`[profile]` → `[knowledge]` → `[server]` → `[agents]` → `[retention]` → `[observation]` → Advanced block: `[confidence]` + operator `[inference]` → optional internal `[inference]` fields

### Consequences

- Implementer has a pre-verified reference table; SR-01 risk is mitigated.
- The two-site default problem (serde fn vs. Rust Default) is documented. For `boosted_categories` and `adaptive_categories`, the config.toml must show the serde default value (`["lesson-learned"]`) since that governs TOML omission behavior.
- `rayon_pool_size` cannot be shown as a single integer in the config comment — the implementer must note it is dynamic and show the formula.
- `ConfidenceWeights` has no `Default` impl intentionally; the config.toml must show all 6 fields as an example block when `preset = "custom"`.
