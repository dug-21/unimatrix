# crt-042: Eval Profile — Pseudocode / Specification

## Purpose

Profile A for the crt-042 A/B eval gate. Enables the expander at default depth and cap.
Profile B (baseline) is the existing `conf-boost-c.toml`.

The profile file is committed to the harness profiles directory. It requires no code change to
`run_eval.py` — the `[inference]` section maps directly to `InferenceConfig` fields via the
same serde deserialization path used by all existing profiles (AC-22).

---

## File Location

```
product/research/ass-037/harness/profiles/ppr-expander-enabled.toml
```

---

## File Content

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

### Field Notes

- `distribution_change = true` — this profile changes the result distribution (new entries
  enter the pool). Annotated truthfully so the eval harness can flag it if distribution-change
  profiles are treated differently in aggregation.
- All other `[inference]` fields are omitted — they pick up the server's configured defaults.
  The eval harness loads this TOML as a project-level config override merged with the running
  server's global config (three-level merge). Omitted fields keep the global value.
- `expansion_depth = 2` and `max_expansion_candidates = 200` match the defaults but are
  explicit here to make the eval parameters self-documenting and to prevent surprises if the
  defaults change in a future feature.

---

## Format Constraints

- Matches the format of existing profiles: two TOML sections `[profile]` and `[inference]`.
- Does not use inline tables or arrays — plain key = value pairs only.
- No trailing whitespace, no blank lines within sections.
- Must be parseable by `run_eval.py --profile ppr-expander-enabled.toml` without modification
  to the eval harness (AC-22).

---

## Eval Gate

| Metric | Gate Condition | Meaning |
|--------|---------------|---------|
| MRR | >= 0.2856 | No regression vs. conf-boost-c.toml baseline |
| P@5 | > 0.1115 | Evidence cross-category entries are now reachable |
| P95 Phase 0 latency | <= 50ms delta over baseline | Measured from `debug!` traces (ADR-005) |

The latency delta is measured by:
1. Running `run_eval.py --profile conf-boost-c.toml` with `RUST_LOG=..search=debug` to
   establish the baseline P95 (expander disabled).
2. Running `run_eval.py --profile ppr-expander-enabled.toml` with `RUST_LOG=..search=debug`.
3. Extracting `elapsed_ms` from Phase 0 `debug!` events in the second run's log.
4. Computing P95 of `elapsed_ms` values. Gate: P95 <= 50ms.

The eval snapshot must be taken AFTER any S1/S2 back-fill migration completes (R-06).

---

## Key Test Scenarios

**AC-22 — profile parses and runs.**
`run_eval.py --profile ppr-expander-enabled.toml` executes to completion without TOML parse
error or config validation error. Eval produces numeric output for MRR and P@5.

**AC-23 — eval gate measurement.**
Run profile. Assert MRR >= 0.2856 (no regression). Record P@5 value. Any P@5 > 0.1115 is
the improvement signal. Record elapsed_ms P95 from Phase 0 debug traces. Compare to baseline.
