# ppr-expander-enabled.toml — Pseudocode
# File: product/research/ass-037/harness/profiles/ppr-expander-enabled.toml

## Purpose

Fix the malformed `ppr-expander-enabled.toml` that causes `parse_profile_toml()` to return
`EvalError::ConfigInvariant` at profile load time, before any graph code is reached.

Root cause: `distribution_change = true` requires all three `[profile.distribution_targets]`
sub-fields (`cc_at_k_min`, `icd_min`, `mrr_floor`). The file declares `distribution_change =
true` but omits the entire `[profile.distribution_targets]` section.

Fix: set `distribution_change = false` (ADR-005, C-06). Add `mrr_floor` and `p_at_5_min`
gates. Add an explanatory comment to prevent regression (SR-04).

---

## Current Content (broken)

```toml
[profile]
name = "ppr-expander-enabled"
description = "PPR expander enabled (crt-042). HNSW k=20 seeds -> graph_expand depth=2 max=200 -> expanded pool -> PPR -> fused scoring."
distribution_change = true

[inference]
ppr_expander_enabled = true
expansion_depth = 2
max_expansion_candidates = 200
```

Problem: `distribution_change = true` with no `[profile.distribution_targets]` block causes
`EvalError::ConfigInvariant` from `parse_profile_toml()`.

---

## Required Content (fixed)

The delivery agent MUST write the following TOML exactly. All values are human-approved
(OQ-01, C-06). Do not alter `mrr_floor`, `p_at_5_min`, `expansion_depth`, or
`max_expansion_candidates` without a scope variance flag.

```toml
[profile]
name = "ppr-expander-enabled"
description = "PPR expander enabled (crt-042/crt-045). HNSW k=20 seeds -> graph_expand depth=2 max=200 -> expanded pool -> PPR -> fused scoring."
# distribution_change = false intentionally.
# CC@k and ICD floors cannot be set without a first-run measurement against this profile.
# Gate on mrr_floor and p_at_5_min only until baseline data is collected.
# See crt-045 ADR-005 and SCOPE.md OQ-01.
distribution_change = false
mrr_floor = 0.2651
p_at_5_min = 0.1083

[inference]
ppr_expander_enabled = true
expansion_depth = 2
max_expansion_candidates = 200
```

---

## Field-by-Field Rationale

| Field | Value | Rationale |
|-------|-------|-----------|
| `name` | `"ppr-expander-enabled"` | Unchanged; used as label in ProfileResult |
| `description` | updated string | Add crt-045 reference; no functional effect |
| `distribution_change` | `false` | Fix parse error (ADR-005). CC@k/ICD deferred. |
| `mrr_floor` | `0.2651` | No-regression gate from crt-042 baseline (OQ-01, C-06) |
| `p_at_5_min` | `0.1083` | First-run improvement gate for PPR/graph_expand (OQ-01, C-06) |
| `ppr_expander_enabled` | `true` | Activates Phase 0 (graph_expand) and Phase 1 (PPR) in search.rs |
| `expansion_depth` | `2` | Unchanged from current file; crt-042 default |
| `max_expansion_candidates` | `200` | Unchanged from current file; crt-042 default |

---

## Structural Notes

### `[profile.distribution_targets]` block

When `distribution_change = false`, the `[profile.distribution_targets]` sub-table and its
fields (`cc_at_k_min`, `icd_min`) MUST NOT be present in the file. Their presence when
`distribution_change = false` is structurally optional in `parse_profile_toml()` (EC-05:
targets are ignored when the flag is false) but omitting them makes intent clear.

### `mrr_floor` and `p_at_5_min` placement

These fields belong in the `[profile]` section (not `[profile.gates]` — verify the actual
TOML schema in `eval/profile/types.rs` before placing them). Based on the architecture
documents and existing profile files, these are top-level `[profile]` fields. The delivery
agent must confirm the field names match `EvalProfile` deserialization before committing.

To verify: `grep -n "mrr_floor\|p_at_5_min" crates/unimatrix-server/src/eval/profile/types.rs`
and check the `EvalProfile` or `EvalProfileConfig` struct definition. Adjust placement if
these fields live in a nested struct.

---

## Parse Validation

After applying the fix, run the following to confirm no parse error:

```
unimatrix eval run --profile ppr-expander-enabled.toml --dry-run
```

Or write a unit test:

```
GIVEN: parse_profile_toml() called with the fixed file content
WHEN:  parsing succeeds
THEN:
  profile.distribution_change == false
  profile.config_overrides.inference.ppr_expander_enabled == true
  profile.mrr_floor == Some(0.2651)        // if field is Option<f64>
  profile.p_at_5_min == Some(0.1083)       // if field is Option<f64>
```

This unit test is part of the R-05 coverage (AC-03).

---

## Error Handling

No runtime error handling — this is a static TOML file. Parse errors are caught by
`parse_profile_toml()` at `eval run` startup. The fix eliminates the only known parse error
(missing `distribution_targets` when `distribution_change = true`).

---

## Key Test Scenarios

1. **AC-03:** `parse_profile_toml()` on the fixed file returns `Ok(profile)` with
   `distribution_change = false` and `ppr_expander_enabled = true`.

2. **Regression guard (SR-04):** If a future edit sets `distribution_change = true` without
   adding `[profile.distribution_targets]`, `parse_profile_toml()` returns
   `EvalError::ConfigInvariant`. The TOML comment is the human-readable guard.

3. **Gate values (C-06):** `mrr_floor = 0.2651` and `p_at_5_min = 0.1083` are exact. Any
   deviation requires a scope variance flag per SCOPE.md OQ-01.
