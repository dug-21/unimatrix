# nan-011-agent-4-config-toml — Implementation Report

## Files Modified

- `/workspaces/unimatrix/config.toml` — full rewrite (8 sections)

## Verification Results

| Check | Result |
|-------|--------|
| TOML validity (`python3 tomllib.load`) | PASS — no exceptions |
| All 8 sections present | PASS |
| boosted_categories serde default | ["lesson-learned"] ✓ (not []) |
| adaptive_categories serde default | ["lesson-learned"] ✓ (not []) |
| rayon_pool_size | formula comment only, no bare integer ✓ |
| NLI block present but fully commented out | PASS |
| confidence.weights sum | 0.20+0.18+0.16+0.15+0.15+0.08 = 0.92 ✓ |

## Defaults Verified from config.rs

| Field | Verified Value | Source |
|-------|---------------|--------|
| `preset` | `"collaborative"` | `#[default]` on `Preset::Collaborative` |
| `categories` | `["lesson-learned","decision","convention","pattern","procedure"]` | `KnowledgeConfig::default()` / `INITIAL_CATEGORIES` |
| `boosted_categories` | `["lesson-learned"]` | `default_boosted_categories()` serde fn |
| `adaptive_categories` | `["lesson-learned"]` | `default_adaptive_categories()` serde fn |
| `default_trust` | `"permissive"` | `AgentsConfig::default()` |
| `session_capabilities` | `["Read","Write","Search"]` | `AgentsConfig::default()` (capitals confirmed) |
| `activity_detail_retention_cycles` | `50` | `default_activity_detail_retention_cycles()` |
| `audit_log_retention_days` | `180` | `default_audit_log_retention_days()` |
| `max_cycles_per_tick` | `10` | `default_max_cycles_per_tick()` |
| `rayon_pool_size` | dynamic `(num_cpus / 2).max(4).min(8)` | `InferenceConfig::default()` — comment only |
| `phase_freq_lookback_days` | `30` | `default_phase_freq_lookback_days()` |
| `min_phase_session_pairs` | `5` | `default_min_phase_session_pairs()` |
| `nli_enabled` | `false` | `default_nli_enabled()` — fully commented out |
| `nli_top_k` | `20` | `default_nli_top_k()` — fully commented out |
| `nli_entailment_threshold` | `0.6` | `default_nli_entailment_threshold()` — 0.6, not 0.5 as SPEC stated |
| `nli_contradiction_threshold` | `0.6` | `default_nli_contradiction_threshold()` |
| `ppr_alpha` | `0.85` | `default_ppr_alpha()` — internal block |
| `ppr_iterations` | `20` | `default_ppr_iterations()` — internal block |
| `ppr_blend_weight` | `0.15` | `default_ppr_blend_weight()` — internal block |
| `ppr_expander_enabled` | `false` | `default_ppr_expander_enabled()` — internal block |
| `supports_cosine_threshold` | `0.65` | `default_supports_cosine_threshold()` — internal block |
| `w_sim` | `0.50` | `default_w_sim()` — internal block |
| `w_conf` | `0.35` | `default_w_conf()` — internal block |
| `w_phase_histogram` | `0.02` | `default_w_phase_histogram()` — internal block |
| `w_phase_explicit` | `0.05` | `default_w_phase_explicit()` — internal block |

## Discrepancy Found

SPECIFICATION.md stated `nli_entailment_threshold` default is 0.5. `config.rs` authority
(`default_nli_entailment_threshold()`) returns 0.6. Used 0.6 per ADR-002 constraint
that config.rs is authoritative. Gate 3a Warning 1 flagged this; pseudocode correctly
showed 0.6.

## Issues

None. All parse checks passed.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entry #3817 (two-site atomicity
  pattern) and #3773 (Default impl literal duplication); both relevant to the serde-vs-Default
  trap.
- Stored: entry #4269 "config.toml must show serde default_fn value, not Rust Default::default()"
  via `/uni-store-pattern` — distinct from #3817 which covers atomic change discipline; this
  covers documentation fidelity when the two sites intentionally diverge.
