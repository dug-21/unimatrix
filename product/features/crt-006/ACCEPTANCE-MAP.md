# crt-006 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | `unimatrix-adapt` crate exists with `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89 | file-check + grep | `test -f crates/unimatrix-adapt/Cargo.toml && grep 'forbid(unsafe_code)' crates/unimatrix-adapt/src/lib.rs` | PENDING |
| AC-02 | MicroLoRA with configurable rank (2-16, default 4) and dimension (default 384) | test | `cargo test -p unimatrix-adapt lora::tests::test_configurable_rank` | PENDING |
| AC-03 | Forward pass: `output = normalize(input + scale * (input @ A @ B))` | test | `cargo test -p unimatrix-adapt lora::tests::test_forward_pass` | PENDING |
| AC-04 | Backward pass computes gradients for A and B | test | `cargo test -p unimatrix-adapt lora::tests::test_gradient_correctness` | PENDING |
| AC-05 | Xavier init for A, near-zero for B (near-identity output) | test | `cargo test -p unimatrix-adapt lora::tests::test_near_identity` | PENDING |
| AC-06 | LoRA+ lr ratio: B lr = ratio * A lr (default 16) | test | `cargo test -p unimatrix-adapt training::tests::test_lora_plus_lr` | PENDING |
| AC-07 | InfoNCE loss with log-sum-exp, temperature default 0.07 | test | `cargo test -p unimatrix-adapt training::tests::test_infonce_loss` | PENDING |
| AC-08 | Training reservoir capacity 512, reservoir sampling | test | `cargo test -p unimatrix-adapt training::tests::test_reservoir_sampling` | PENDING |
| AC-09 | Configurable batch size (default 32), partial batch handling | test | `cargo test -p unimatrix-adapt training::tests::test_partial_batch` | PENDING |
| AC-10 | Within-batch negative sampling | test | `cargo test -p unimatrix-adapt training::tests::test_within_batch_negatives` | PENDING |
| AC-11 | EWC++ online Fisher diagonal, alpha=0.95 | test | `cargo test -p unimatrix-adapt regularization::tests::test_fisher_update` | PENDING |
| AC-12 | EWC penalty: L_total = L_infonce + (lambda/2) * sum(F_i * (theta_i - theta*_i)^2) | test | `cargo test -p unimatrix-adapt regularization::tests::test_ewc_penalty` | PENDING |
| AC-13 | Prototype centroids bounded max 256, min entries 3 | test | `cargo test -p unimatrix-adapt prototypes::tests::test_bounds` | PENDING |
| AC-14 | Prototype soft pull formula | test | `cargo test -p unimatrix-adapt prototypes::tests::test_soft_pull` | PENDING |
| AC-15 | Prototype LRU eviction | test | `cargo test -p unimatrix-adapt prototypes::tests::test_lru_eviction` | PENDING |
| AC-16 | Episodic augmentation for search refinement | test | `cargo test -p unimatrix-adapt episodic::tests::test_augmentation` | PENDING |
| AC-17 | All entry embedding ops use adaptation | test | integration test A-03 or server-level test | PENDING |
| AC-18 | Query embeddings adapted before HNSW search | test | integration test verifying adapted query | PENDING |
| AC-19 | Adaptation state persisted alongside HNSW dump | test | `cargo test -p unimatrix-adapt persistence::tests::test_save_load` | PENDING |
| AC-20 | State version field for forward-compatible evolution | test | `cargo test -p unimatrix-adapt persistence::tests::test_version_compat` | PENDING |
| AC-21 | Training generation counter | test | `cargo test -p unimatrix-adapt service::tests::test_generation_counter` | PENDING |
| AC-22 | Training triggered inline during co-access recording | test | integration test verifying training triggers | PENDING |
| AC-23 | Training time bounded regardless of KB size | test | `cargo test -p unimatrix-adapt training::tests::test_bounded_time` | PENDING |
| AC-24 | Embedding consistency check uses adapted re-embeddings | test | integration test A-04 | PENDING |
| AC-25 | Pre-allocated buffers for forward pass | test | benchmark or allocation-counting test | PENDING |
| AC-26 | No external ML framework dependencies | grep | `grep -L 'torch\|candle\|burn' crates/unimatrix-adapt/Cargo.toml` | PENDING |
| AC-27 | ndarray as dependency | file-check | `grep ndarray crates/unimatrix-adapt/Cargo.toml` | PENDING |
| AC-28 | Unit tests for all components | test | `cargo test -p unimatrix-adapt --lib` passes | PENDING |
| AC-29 | Integration tests for end-to-end flow | test | integration suite passes | PENDING |
| AC-30 | Scale tests for 10K+ simulated entries | test | parameter count + memory bound assertions | PENDING |
| AC-31 | Existing tests pass (no regressions) | test | `cargo test` + full integration suite (157+ tests) | PENDING |
| AC-32 | `#![forbid(unsafe_code)]` in all crates | grep | `grep -r 'forbid(unsafe_code)' crates/*/src/lib.rs` | PENDING |
| AC-33 | New test_adaptation.py integration suite | file-check | `test -f product/test/infra-001/suites/test_adaptation.py` | PASS |
| AC-34 | Adaptation state persistence across restart | test | integration test A-02 | PASS |
| AC-35 | Embedding consistency with adaptation active | test | integration test A-04 | PASS |
| AC-36 | Adapted search quality verification | test | integration test A-03 | PASS |
| AC-37 | Cold-start near-identity behavior | test | integration test A-01 | PASS |
| AC-38 | Volume suite unchanged with adaptation | test | `pytest suites/test_volume.py` passes | PASS |
| AC-39 | Smoke test covers adaptation path | test | `pytest -m smoke` includes A-01 | PASS |
