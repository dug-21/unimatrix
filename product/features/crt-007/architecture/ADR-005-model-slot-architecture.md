## ADR-005: Three-Slot Model Versioning

### Context

Neural models evolve over time (crt-008 retraining). The system needs safe deployment: new models must prove themselves before replacing production, and production must be recoverable if a new model regresses.

Options:
- **a) Single slot (overwrite)**: Simplest. No rollback capability. Regression requires manual cold-restart.
- **b) Two slots (production + previous)**: Rollback possible but no shadow evaluation period.
- **c) Three slots (production + shadow + previous)**: Shadow for evaluation, production for active inference, previous for rollback.
- **d) N-slot history**: Full version history. Overkill for small models; adds storage and complexity.

### Decision

Three slots per model name: Production, Shadow, Previous. State transitions:

```
Cold Start:  -> Production (baseline weights)

Promotion:   Shadow -> Production
             (old) Production -> Previous
             (old) Previous deleted

Rollback:    Production -> Shadow
             Previous -> Production

Cold Restart: -> Shadow (baseline weights)
              Production and Previous unchanged
```

Promotion criteria (all must hold):
1. Shadow accuracy >= production accuracy (on the evaluation window)
2. Minimum 20 evaluations in shadow
3. No per-category regression (each category accuracy >= production's)

Rollback trigger:
- Rolling accuracy (window of 50 predictions) drops > 5% below the accuracy at promotion time

Registry state persisted as JSON at `~/.unimatrix/{project_hash}/models/registry.json`. Model weights persisted via burn's record system at `~/.unimatrix/{project_hash}/models/{model_name}/{slot}.bin`.

### Consequences

- **Easier**: Safe deployment with automatic rollback. Shadow evaluation before any production impact. Single previous slot sufficient for immediate recovery.
- **Harder**: Three copies of model weights in memory/disk. Registry state management adds ~50 lines.
- **Mitigated**: Models are small (< 7MB total). Three copies = ~21MB disk, negligible. Registry JSON is < 1KB.
