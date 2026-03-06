## ADR-001: burn Framework for Neural Models

### Context

crt-007 requires neural network models (MLP classifiers) with three key properties: (1) full training loop control for EWC gradient injection (crt-008), (2) autodiff for backpropagation through custom loss terms, (3) support for shared-weight architectures like Siamese MLPs (crt-009 Duplicate Detector). The original scope specified ruv-fann, but its black-box RPROP training prevents all three.

Three options evaluated:
- **ruv-fann**: RPROP optimizer is opaque -- cannot inject EWC gradients, cannot customize training loop, cannot share weights. Exit ramp exercised.
- **ndarray + hand-rolled training**: Full control but requires manual backpropagation, gradient computation, optimizer implementation. High maintenance burden for crt-008/009.
- **burn**: Full training loop ownership via `TrainStep` trait, native autodiff backend, declarative module system, burn-ndarray CPU backend. Pure Rust, MIT/Apache-2.0.

### Decision

Use burn 0.16 with the NdArray CPU backend (`burn-ndarray`) for all neural models in unimatrix-learn. Pin the exact burn version in workspace dependencies. Models defined as burn modules (`nn::Linear`, activation functions). Inference uses the base backend; training (crt-008) will use the autodiff backend.

ndarray 0.16 remains in unimatrix-learn for shared infrastructure (EwcState, TrainingReservoir) where full tensor/autodiff is unnecessary. The boundary between ndarray and burn is `Vec<f32>` flat parameter vectors.

### Consequences

- **Easier**: Training loop customization (EWC injection, custom loss), model definition (declarative), crt-009 Siamese architecture, ONNX export for model inspection.
- **Harder**: Binary size increases (~5-15MB for burn + burn-ndarray). Two math libraries in the workspace (ndarray for existing infra, burn for new models). burn API may evolve between minor versions.
- **Mitigated**: Pin exact burn version. Feature-gate burn if binary size exceeds 15MB (SR-01). Model files include burn_version in metadata for compatibility detection (SR-03).
