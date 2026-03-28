# Security Review: bugfix-434-security-reviewer

## Risk Level: low

## Summary

The change lowers the `supports_edge_threshold` default from 0.7 to 0.6 in `InferenceConfig`. This is a pure constant adjustment to a configuration value that controls how aggressively the background graph inference tick writes NLI-derived "Supports" edges. No new code paths, no new inputs, no new trust boundaries, no new dependencies. All existing validation logic (range checks, cross-field invariant) remains intact and still applies to the new value.

## Findings

### Finding 1: No OWASP-Relevant Attack Surface
- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/infra/config.rs:386–393, 465, 563–565`
- **Description**: The change touches only a compile-time constant and its serde default function. The value is consumed at tick time by the NLI graph inference pipeline. No external input flows through `supports_edge_threshold` at runtime — it is set once from the TOML config file (operator-controlled, not user-controlled) and validated before use by `InferenceConfig::validate()`.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 2: Cross-Field Invariant Preserved
- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/infra/config.rs:719–727`
- **Description**: The fix was verified against the cross-field invariant `supports_candidate_threshold < supports_edge_threshold` (enforced by `validate()`). The new default pairing is 0.5 < 0.6, which satisfies the strict inequality. The boundary-violation tests (`test_validate_rejects_equal_thresholds`, `test_validate_rejects_candidate_above_edge`) were correctly left untouched — they use explicit struct spreads at 0.7, testing the validation logic rather than the default value.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 3: Regression Test Adequacy
- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/infra/config.rs:4795–4806`
- **Description**: The new regression guard (`test_write_inferred_edges_default_threshold_yields_edges_at_0_6`) asserts `InferenceConfig::default().supports_edge_threshold < 0.7_f32`. This is a weaker assertion than an equality check — it would pass for any value in (0.0, 0.7), not exclusively 0.6. If a future change accidentally lowers the threshold further (e.g., to 0.5, equal to `supports_candidate_threshold`), this test would still pass while the runtime validation would then catch the invariant violation.
- **Recommendation**: The equality assertions in `test_inference_config_defaults` and `test_inference_config_toml_defaults` already pin the value to exactly 0.6, so the regression is adequately caught. The weaker `< 0.7` guard is supplementary. No change needed; the gap is fully covered.
- **Blocking**: no

### Finding 4: No New Dependencies
- **Severity**: low (informational)
- **Description**: The diff introduces no new crate dependencies. `Cargo.toml` and `Cargo.lock` are not modified. No CVE exposure introduced.
- **Blocking**: no

### Finding 5: No Secrets or Credentials
- **Severity**: low (informational)
- **Description**: No hardcoded tokens, API keys, passwords, or sensitive values in the diff. Changed values are floating-point thresholds.
- **Blocking**: no

## Blast Radius Assessment

**Worst case scenario**: The new default (0.6) is equal to `nli_entailment_threshold` (also 0.6, the post-store path). If the NLI model is biased and produces systematically high entailment scores, the graph tick would write more Supports edges than intended. The runtime safeguard is `max_graph_inference_per_tick: 100`, which caps the tick budget per cycle. The validator enforces 0.5 < 0.6 so the cross-field invariant holds. The failure mode if the value was set subtly wrong would be: more graph edges written per tick (higher connectivity), which degrades to slightly noisier search re-ranking, not data corruption or information disclosure. This is a safe failure mode — no silent corruption, no privilege escalation.

## Regression Risk

**Low.** The change affects only the graph inference background tick path. The post-store NLI path (`nli_entailment_threshold`) is independent and unchanged. The integration lifecycle suite (`test_post_store_nli_edge_written`, `test_search_nli_absent_returns_cosine_results`) and smoke suite (20/20) all passed. The validation boundary tests were correctly preserved. Existing TOML configs that explicitly set `supports_edge_threshold = 0.7` are not affected — explicit values override the default via serde.

## PR Comments
- Posted 1 comment on PR #435 (see below)
- Blocking findings: no

## Knowledge Stewardship
- nothing novel to store — the security properties of this fix are straightforward (constant-only change, existing validation preserved, no new trust boundaries). No recurring anti-pattern detected.
