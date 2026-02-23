## ADR-001: Mutex\<Session\> for ONNX Inference Concurrency

### Context

The `ort::Session::run()` method requires `&mut self` -- it takes a mutable borrow of the session for inference. This means a single ONNX session cannot serve concurrent inference requests without synchronization.

We need `OnnxProvider` to be `Send + Sync` so it can be shared via `Arc<OnnxProvider>` across threads (matching the `Arc<Store>` and `Arc<VectorIndex>` patterns from nxs-001 and nxs-002).

Options considered:
1. **Mutex\<Session\>** -- serialize inference calls behind a mutex.
2. **RwLock\<Session\>** -- not applicable because `run()` needs `&mut self` (write lock every time), making RwLock equivalent to Mutex but with higher overhead.
3. **Session pool** -- create multiple sessions and hand them out on demand. Higher memory usage (~90MB per session) and added complexity.
4. **Per-thread sessions** -- each thread creates its own session. Same memory concern as pooling.

### Decision

Use `Mutex<ort::Session>` inside `OnnxProvider`. The `tokenizers::Tokenizer` remains outside the mutex because `Tokenizer::encode()` takes `&self` and is safe for concurrent use.

```rust
pub struct OnnxProvider {
    session: Mutex<ort::Session>,   // locked only during inference
    tokenizer: tokenizers::Tokenizer, // lock-free, &self methods
    model: EmbeddingModel,
    config: EmbedConfig,
}
```

Inference flow:
1. Tokenize input (no lock needed).
2. Lock session mutex.
3. Run ONNX inference.
4. Unlock session mutex.
5. Pool and normalize (no lock needed).

### Consequences

**Easier:**
- `OnnxProvider` is `Send + Sync` with minimal complexity.
- Single session means single ~90MB memory footprint for the model.
- Matches the simplicity-first philosophy of nxs-001/nxs-002.
- Batch embedding amortizes the lock -- one lock for N texts.

**Harder:**
- Concurrent inference requests are serialized. Under heavy concurrent load, callers queue at the mutex.
- Mitigation: For workloads that need parallel inference, create multiple `OnnxProvider` instances. This is a future optimization, not needed for Unimatrix's expected load (single-agent write path, sequential MCP tool calls).
