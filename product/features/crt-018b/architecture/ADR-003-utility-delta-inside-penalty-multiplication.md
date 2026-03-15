## ADR-003: Utility Delta Applied Inside the Status Penalty Multiplication

### Context

The existing search re-ranking formula applies status penalties (Deprecated: 0.7x,
Superseded: 0.5x) multiplicatively to the base score:

```
final_score = (rerank_score(sim, conf, cw) + prov_boost + co_access_boost) * status_penalty
```

The utility delta (+0.05, 0.0, -0.05, +0.01) must be positioned somewhere in this formula.
Two positions are architecturally meaningful:

1. **Inside** (same multiplicand as the base score):
   ```
   final_score = (rerank_score(sim, conf, cw) + utility_delta + prov_boost + co_access_boost) * status_penalty
   ```

2. **Outside** (bypasses the penalty multiplier):
   ```
   final_score = (rerank_score(sim, conf, cw) + prov_boost + co_access_boost) * status_penalty + utility_delta
   ```

The practical difference: for a Deprecated entry (penalty 0.7) with `Effective` classification,
placing the delta inside means the boost is reduced by the penalty (0.7 * 0.05 = 0.035 net boost).
Placing it outside means the full 0.05 boost applies regardless of deprecation status.

Additionally, SR-04 raises the question of how the utility delta interacts with the adaptive
confidence weight from crt-019. At minimum confidence weight (0.15), similarity dominates. The
utility delta magnitude (0.05) is proportionally larger relative to the confidence term (0.15 * conf)
than at full weight (0.25 * conf). This interaction must be accounted for in the formula design.

Verified range analysis at minimum confidence weight (0.15):
- sim=0.75, conf=0.60: rerank_score = 0.85*0.75 + 0.15*0.60 = 0.7275
- With +0.05 Effective delta: 0.7775 (+6.9% relative)
- With -0.05 Ineffective delta: 0.6775 (-6.9% relative)
- A highly similar Ineffective entry (sim=0.95, conf=0.60): 0.895 - 0.05 = 0.845
- A lower-similarity Effective entry (sim=0.75, conf=0.60): 0.7275 + 0.05 = 0.7775
- Result: the highly similar Ineffective entry still surfaces above the lower-similarity Effective
  entry (0.845 > 0.7775). This is correct — the delta displaces ties but does not fully suppress
  a significantly more similar entry.

### Decision

Place the utility delta **inside** the status penalty multiplication, alongside provenance boost and
co-access boost.

Rationale:
1. **Consistency with established signals**: provenance boost and co-access boost are both inside
   the penalty multiplication. Placing utility delta in the same position maintains a single
   conceptual "additive bonus" group that is subject to the same status penalty.
2. **Logical coherence**: a Deprecated entry that happens to be Effective is still deprecated and
   should have its overall score reduced by the deprecation penalty. The effectiveness signal
   should not override the status lifecycle signal.
3. **Simpler formula**: no special-casing for where the delta falls relative to the penalty.
   All additive signals are in one group before the penalty multiplier.

The full formula at all four `rerank_score` call sites in `search.rs` (Steps 7 and 8):

Step 7 (initial sort):
```rust
let base_a = rerank_score(sim_a, entry_a.confidence, confidence_weight)
    + utility_delta(categories.get(&entry_a.id).copied())
    + prov_a;
let final_a = base_a * penalty_a;
```

Step 8 (co-access re-sort):
```rust
let final_a = (rerank_score(sim_a, entry_a.confidence, confidence_weight)
    + utility_delta(categories.get(&entry_a.id).copied())
    + boost_a + prov_a) * penalty_a;
```

### Consequences

Easier:
- Formula is consistent with provenance boost and co-access boost (all inside penalty group).
- Single conceptual "additive bonus" group simplifies test reasoning.
- Status lifecycle signals (deprecated/superseded) continue to dominate effectiveness signals
  for entries that are in terminal lifecycle states.

Harder:
- For Deprecated Effective entries, the Effective boost is dampened by 0.7x (to 0.035 net).
  This may cause a Deprecated Effective entry to rank below an Active Effective entry with lower
  similarity more aggressively than if the boost were outside the penalty. This is the intended
  behavior — a deprecated entry should generally rank lower than an active one regardless of
  effectiveness history — but may surprise operators who expect a fully boosted deprecated entry.
