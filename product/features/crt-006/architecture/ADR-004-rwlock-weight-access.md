## ADR-004: RwLock for Weight Access with Atomic Swap

### Context

MicroLoRA weights are read on every embedding operation (hot path) and written during training steps (background, fire-and-forget). Multiple threads may read simultaneously (multiple concurrent tool calls), while training needs exclusive write access.

Risk SR-05 identified: partial weight updates during training could leave the model in an inconsistent state if training fails mid-update.

### Decision

Use `RwLock<LoraWeights>` where `LoraWeights` contains matrices A, B, and associated metadata. The training pipeline follows an **atomic swap** pattern:

1. Acquire read lock to copy current weights
2. Compute new weights in a local buffer (forward passes, loss, gradients, weight updates) -- no lock held during computation
3. Run NaN/Inf validation on new weights
4. Acquire write lock briefly to swap old weights for new weights
5. Release write lock

If any step fails (NaN detected, computation error), the swap never happens. Weights remain at their pre-step values.

The EWC state, prototype manager, and training reservoir also live behind `RwLock` with the same atomic-update pattern.

### Consequences

- **Easier**: Concurrent reads never block each other. Forward pass latency is unaffected by training.
- **Easier**: Training failures are atomic -- weights are never left in a partial state.
- **Easier**: Standard library primitive, no external concurrency dependency.
- **Harder**: Write lock acquisition adds a brief latency spike for reads that arrive during the swap. At microsecond swap times, this is negligible.
- **Harder**: `RwLock` requires the inner type to be `Send + Sync`, which ndarray arrays satisfy.
