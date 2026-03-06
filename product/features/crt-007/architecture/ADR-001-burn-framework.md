## ADR-001: ndarray-only for Neural Models (supersedes burn)

### Context

crt-007 requires neural network models (MLP classifiers) with three key properties: (1) full training loop control for EWC gradient injection (crt-008), (2) explicit backpropagation through custom loss terms, (3) a trait-based abstraction that can accommodate future transformer models. The original scope specified ruv-fann (exit-ramped), then burn.

Four options evaluated:
- **ruv-fann**: RPROP optimizer is opaque -- cannot inject EWC gradients, cannot customize training loop. Exit ramp exercised.
- **burn**: Full training loop ownership, native autodiff, declarative modules. But: adds 5-15MB binary overhead and a second math library for models that are trivially implementable as matrix multiplies.
- **ndarray + hand-rolled training**: Full control, zero new dependencies, single math library across workspace. MLP forward/backward passes are ~30 lines each for 2-3 layer networks. The "maintenance burden" concern is negligible for shallow MLPs.
- **NeuralModel trait + ndarray now, burn/candle later**: Design the trait abstraction knowing future models may need a framework, but don't pay the dependency cost until those models arrive.

### Decision

Use ndarray 0.16 (already in workspace) with hand-rolled forward/backward passes for all neural models in crt-007/008/009. Define a `NeuralModel` trait that abstracts the model lifecycle:

```rust
pub trait NeuralModel: Send + Sync {
    fn forward(&self, input: &[f32]) -> Vec<f32>;
    fn train_step(&mut self, input: &[f32], target: &[f32], lr: f32) -> f32; // returns loss
    fn flat_parameters(&self) -> Vec<f32>;
    fn set_parameters(&mut self, params: &[f32]);
    fn serialize(&self) -> Vec<u8>;
    fn deserialize(data: &[u8]) -> Result<Self, String> where Self: Sized;
}
```

Signal Classifier and Convention Scorer implement `NeuralModel` via ndarray matrix operations. Future micro-transformer models (crt-009+) can implement `NeuralModel` behind a `feature = "burn"` or `feature = "candle"` cargo feature gate.

**Supersedes**: Previous ADR-001 (burn framework selection, Unimatrix #404, deprecated).

### Consequences

- **Easier**: Zero new dependencies. Single math library (ndarray) across entire workspace. No conversion boundary between frameworks. Leaner binary. Direct EWC gradient injection without framework indirection. Full visibility into forward/backward computation.
- **Harder**: Manual backpropagation for each model type (~30 lines per model). No autodiff safety net -- gradient bugs are possible. Future transformer models will need a framework dependency (deferred cost).
- **Mitigated**: MLP backprop is mathematically simple and well-tested. `NeuralModel` trait provides the abstraction boundary so framework dependencies are additive, not breaking. Known-value gradient tests catch backprop errors.
