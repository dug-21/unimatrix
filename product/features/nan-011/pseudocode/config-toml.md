# Component 2 — config.toml Full Rewrite

## Purpose

Rewrite `config.toml` from its current 26-line `[retention]`-only state to a
complete 8-section reference configuration covering all operator-facing fields.
All default values must be verified against `crates/unimatrix-server/src/infra/config.rs`
`default_*` functions — not assumed from memory.

---

## Pre-Work: Read config.rs Before Writing Any Values

MANDATORY before writing any field:

```
Read: crates/unimatrix-server/src/infra/config.rs
```

Specifically locate and read these functions/impls:
- `default_boosted_categories()` — must return `vec!["lesson-learned"]`
- `default_adaptive_categories()` — must return `vec!["lesson-learned"]`
- `Default` impl for `AgentsConfig` — confirms `"permissive"` and `["Read","Write","Search"]`
- `default_activity_detail_retention_cycles()` — must return `50`
- `default_audit_log_retention_days()` — must return `180`
- `default_max_cycles_per_tick()` — must return `10`
- `Default` impl for `InferenceConfig` — confirms `rayon_pool_size` is dynamic
- `default_phase_freq_lookback_days()` — must return `30`
- `default_min_phase_session_pairs()` — must return `5`
- `default_nli_enabled()` — must return `false`
- `default_nli_top_k()` — must return `20`
- `#[default]` on `Preset::Collaborative` with `#[serde(rename_all = "lowercase")]`

If ANY value read from config.rs differs from ADR-002's table, use the config.rs value.
config.rs is the authority.

Also read: `crates/unimatrix-server/src/infra/categories/mod.rs`
Locate `INITIAL_CATEGORIES` — must be `["lesson-learned","decision","convention","pattern","procedure"]`.

---

## Structure: 8-Section TOML File

Write the file in this exact section order. Do not reorder sections:

```
[profile]
[knowledge]
[server]
[agents]
[retention]
[observation]
# == Advanced Configuration ==
[confidence]
[inference]
```

The advanced block header comment separates operator-standard fields from fields
that require deep understanding to modify.

---

## Section-by-Section Pseudocode

### [profile] Section

```toml
[profile]
# Active configuration preset. Controls how confidence components are weighted
# for knowledge scoring. Accepted values:
#   collaborative  — balanced weights; suited for team-based agentic delivery
#   authoritative  — elevated trust and usage signals; suited for structured pipelines
#   operational    — freshness and correction signals elevated; suited for ops domains
#   empirical      — helpfulness and co-access signals elevated; suited for research
# Default: "collaborative"
preset = "collaborative"

# To use custom confidence weights instead of a preset, set preset = "custom"
# and configure the [confidence] block below.
# preset = "custom"
```

### [knowledge] Section

```toml
[knowledge]
# Active knowledge categories. Defines the allowlist of accepted category values.
# Entries using categories not in this list are rejected at write time.
# Default: the 5 built-in categories
categories = ["lesson-learned", "decision", "convention", "pattern", "procedure"]

# Categories that receive an additional ranking boost in search results.
# NOTE: This field has TWO default sites in config.rs:
#   - When omitted from this file (serde default): ["lesson-learned"]
#   - Programmatic Default::default():             []
#   This file shows the serde default — what you get when you omit the field.
boosted_categories = ["lesson-learned"]

# Categories eligible for adaptive lifecycle management (confidence decay adjustment).
# Same two-site default behavior as boosted_categories.
adaptive_categories = ["lesson-learned"]

# Override the freshness half-life used in confidence scoring.
# When absent, the preset's built-in half-life is used.
# Units: hours (float)
# freshness_half_life_hours = 720.0
```

### [server] Section

```toml
[server]
# Custom system prompt injected into context briefings.
# When absent, the compiled SERVER_INSTRUCTIONS constant is used.
# instructions = "You are an expert in..."
```

### [agents] Section

```toml
[agents]
# Trust level assigned to agents that auto-enroll (not yet in the registry).
# Accepted values: "permissive", "restricted", "admin"
# Default: "permissive"
default_trust = "permissive"

# Capabilities granted per-session to known agents.
# Values are case-sensitive. Accepted: "Read", "Write", "Search", "Admin"
# Default: ["Read", "Write", "Search"]
session_capabilities = ["Read", "Write", "Search"]
```

### [retention] Section

```toml
[retention]
# Number of delivery cycles for which per-cycle activity detail is retained.
# After this many cycles, detail records are pruned; summary data is preserved.
# Default: 50
activity_detail_retention_cycles = 50

# Number of days audit log entries are retained before pruning.
# Default: 180
audit_log_retention_days = 180

# Maximum number of cycles processed per maintenance tick.
# Limits the work done per tick to avoid blocking the query path.
# Default: 10
max_cycles_per_tick = 10
```

### [observation] Section

```toml
[observation]
# Domain pack registration. Each [[observation.domain_packs]] entry connects an
# external event source to Unimatrix's observation pipeline.
#
# The built-in "claude-code" domain pack is always active and requires no
# configuration here.
#
# To add a custom domain, uncomment and complete the example below:
#
# [[observation.domain_packs]]
# # Unique identifier for this domain (string).  REQUIRED — no default.
# source_domain = "my-domain"
#
# # Event type strings accepted from this domain's event stream.  REQUIRED.
# event_types = ["deploy", "test_failure", "incident"]
#
# # Knowledge categories used by this domain's detection rules.  REQUIRED.
# categories = ["lesson-learned", "pattern"]
#
# # Path to a custom detection rule file for this domain.  Optional.
# # Accepts absolute paths or paths relative to the config file location.
# # rule_file = "/path/to/rules.toml"
```

Note: `[[observation.domain_packs]]` is a TOML table-of-tables (double brackets).
The commented example must use `[[...]]`, not `[...]`. Keep both bracket characters
commented out — if only the `#` character is placed on the content lines but the
`[[...]]` line is not commented, the TOML becomes malformed.

### [confidence] Section (Advanced Block)

Begin the advanced block with a clearly-marked header comment:

```toml
# =============================================================================
# Advanced Configuration
# Fields below require deep understanding of Unimatrix's internals to modify.
# Incorrect values can degrade retrieval quality. Change only when directed.
# =============================================================================

[confidence]
# Custom confidence weight components. Active ONLY when preset = "custom".
# All six values must sum to exactly 0.92 (±1e-9). No partial override allowed —
# all six must be specified when this block is uncommented.
#
# Component meanings:
#   base   — baseline trust granted to newly stored entries
#   usage  — weight of retrieval frequency signal
#   fresh  — weight of entry freshness (recency signal)
#   help   — weight of explicit helpfulness votes (context_store feedback)
#   corr   — weight of correction history (context_correct usage)
#   trust  — weight of the storing agent's trust level
#
# Example summing to 0.92:
# [confidence.weights]
# base  = 0.20
# usage = 0.18
# fresh = 0.16
# help  = 0.15
# corr  = 0.15
# trust = 0.08
# # Sum: 0.92 — required
```

VERIFY: The six example values in the comment sum to 0.92. Use exact arithmetic:
0.20 + 0.18 + 0.16 + 0.15 + 0.15 + 0.08 = 0.92. If using different example
values, verify the sum before writing.

### [inference] Section (Advanced Block)

The `[inference]` section has three sub-blocks. Write them in this order:
1. Operator-facing fields (uncommented)
2. NLI opt-in block (fully commented out)
3. Internal tuning block (fully commented out, with "do not change" warning)

```toml
[inference]
# Number of threads in the Rayon thread pool used for vector operations.
# This value is DYNAMICALLY computed at startup: (num_cpus / 2).max(4).min(8)
# Override only if you need to reduce parallelism (e.g., constrained containers).
# rayon_pool_size = 4   # dynamic default shown; actual value depends on hardware

# Number of days of session history used for phase frequency analysis.
# Default: 30
phase_freq_lookback_days = 30

# Minimum number of session pairs required before phase affinity kicks in.
# Below this count, phase affinity scoring is skipped.
# Default: 5
min_phase_session_pairs = 5

# -----------------------------------------------------------------------------
# NLI cross-encoder (opt-in). Requires an external ONNX NLI cross-encoder model
# file. Not bundled with Unimatrix. See documentation for model acquisition.
# -----------------------------------------------------------------------------
# nli_enabled = false
# nli_model_name = "NliMiniLM2L6H768Q8"
# nli_model_path = "/path/to/model.onnx"
# nli_model_sha256 = ""
# nli_top_k = 20
# nli_entailment_threshold = 0.6
# nli_contradiction_threshold = 0.6

# -----------------------------------------------------------------------------
# Internal tuning — do not change unless directed by a support issue.
# These values control the retrieval fusion weights and graph traversal algorithm.
# Incorrect values can silently degrade knowledge quality.
# -----------------------------------------------------------------------------
# ppr_alpha = 0.85
# ppr_iterations = 20
# ppr_inclusion_threshold = 0.05
# ppr_blend_weight = 0.15
# ppr_max_expand = 50
# ppr_expander_enabled = false
# w_sim = 0.50
# w_conf = 0.35
# w_phase_histogram = 0.02
# w_phase_explicit = 0.05
# supports_cosine_threshold = 0.65
```

CRITICAL: `rayon_pool_size` must NOT appear as a bare uncommented integer. It must
appear only in a comment line showing the formula and explaining it is dynamic.

---

## Constraints Encoded in the Output File

1. Every uncommented field has a comment explaining purpose, accepted values, and default.
2. `boosted_categories` and `adaptive_categories` have a comment explaining the two-site
   default distinction (serde fn vs. Rust Default::default()).
3. `rayon_pool_size` appears only in a comment, never as an uncommented TOML field.
4. The NLI sub-block is fully commented out — every line prefixed with `#`.
5. The `[[observation.domain_packs]]` example is fully commented out.
6. The `[confidence.weights]` block is commented out (only active when preset = "custom").
7. `ConfidenceWeights` example values sum to 0.92.
8. `session_capabilities` uses capital R/W/S: `["Read", "Write", "Search"]`.
9. `source_domain`, `event_types`, `categories` in the domain_packs example are labeled REQUIRED.
10. The path note for `rule_file` and `nli_model_path` mentions accepting absolute or
    relative paths (security comment per RISK-TEST-STRATEGY.md).

---

## Verification Steps

After writing the file:

```bash
# 1. TOML validity (main form — all optional blocks commented)
python3 -c "import tomllib; tomllib.load(open('config.toml','rb'))"
# Must produce no errors.

# 2. Temporarily uncomment [[observation.domain_packs]] example — re-run parser
# (manually uncomment, parse, re-comment)

# 3. Temporarily uncomment [confidence.weights] block — re-run parser
# (verify weights sum to 0.92 in the comment arithmetic)

# 4. Temporarily uncomment NLI block — re-run parser

# 5. Confirm boosted_categories value
grep 'boosted_categories' config.toml | grep '"lesson-learned"'
# Must match — not []

# 6. Confirm adaptive_categories value
grep 'adaptive_categories' config.toml | grep '"lesson-learned"'
# Must match — not []

# 7. Confirm rayon_pool_size is not an uncommented integer
grep '^rayon_pool_size' config.toml
# Must return zero matches (the field must not appear uncommented)
```

---

## Error Handling

If config.rs default values differ from ADR-002 table: use config.rs, document
the discrepancy in the agent report.

If `INITIAL_CATEGORIES` in categories/mod.rs has changed: update `categories`
field accordingly and document in the agent report.

Do not leave any uncommented field without a comment — AC-06 requires every
uncommented field to have at least one comment.

---

## Key Test Scenarios

1. TOML parse: `python3 -c "import tomllib; tomllib.load(open('config.toml','rb'))"` — no error.
2. All 8 sections present: grep for `[profile]`, `[knowledge]`, `[server]`, `[agents]`,
   `[retention]`, `[observation]`, `[confidence]`, `[inference]`.
3. boosted_categories = `["lesson-learned"]` (not `[]`).
4. adaptive_categories = `["lesson-learned"]` (not `[]`).
5. rayon_pool_size not uncommented: `grep '^rayon_pool_size' config.toml` returns zero.
6. NLI block fully commented: `grep '^nli_enabled' config.toml` returns zero.
7. domain_packs example is commented: `grep '^\[\[observation' config.toml` returns zero.
8. ConfidenceWeights example sums to 0.92.
9. session_capabilities uses capital letters: grep for `"Read"`, `"Write"`, `"Search"`.
