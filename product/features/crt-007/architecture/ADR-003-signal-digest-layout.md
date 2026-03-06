## ADR-003: Fixed-Width 32-Slot SignalDigest

### Context

Neural models need a structured input vector derived from ProposedEntry fields and store queries. The feature set will grow across crt-007 (6-7 initial features), crt-008 (training signals), and crt-009 (advanced features). Changing the input dimensionality requires retraining or discarding learned weights.

Options:
- **a) Variable-width with schema version**: Flexible but requires model migration on every new feature. A self-learning system that goes dumb on schema changes is not self-learning.
- **b) Fixed-width with reserved slots (32 floats)**: crt-007 uses ~7 slots; remainder zero-initialized. New features fill empty slots additively. No model retraining required for additive changes.
- **c) Large fixed-width (128/256)**: More headroom but zero-padding may dominate training dynamics with only 7 active features.

### Decision

`SignalDigest` is `[f32; 32]`. Slots 0-6 are assigned in crt-007. Slots 7-31 are reserved, zero-initialized. Known roadmap needs ~15 features; 32 provides headroom. Power-of-2 aligns with SIMD/cache.

Slot assignment table:
| Slot | Feature | Normalization |
|------|---------|---------------|
| 0 | extraction_confidence | Already [0,1] |
| 1 | source_feature_count | / 10.0, clamped [0,1] |
| 2 | content_length_norm | / 1000.0, clamped [0,1] |
| 3 | category_idx | Ordinal / 5.0 |
| 4 | rule_idx | Ordinal / 5.0 |
| 5 | title_length_norm | / 200.0, clamped [0,1] |
| 6 | tag_count_norm | / 10.0, clamped [0,1] |
| 7-31 | Reserved | 0.0 |

Breaking change fallback (removing/reordering features): ModelRegistry detects schema version mismatch, demotes old model, cold-starts with conservative bias weights.

### Consequences

- **Easier**: Additive feature growth without model retraining. Stable topology across crt-007/008/009.
- **Harder**: ~78% of input is zeros initially. Must verify zero-padding does not dominate gradient flow for small active feature counts (SR-06).
- **Mitigated**: Hand-tuned baseline weights are designed for the 7-active-slot configuration. Smoke test validates non-degenerate output.
