# ASS-046: GGUF Feasibility

**Date**: 2026-04-09
**Tier**: 3 (deferred — does not block any other Wave 2 item)
**Feeds**: W2-5 (GGUF module — go/no-go)

---

## Question

Is local GGUF inference via llama.cpp viable in the Unimatrix daemon process for the target deployment environments, and if so, which Rust FFI crate and model class should be used?

---

## Why It Matters

W2-5 is conditional on this spike. Committing to llama.cpp FFI without a proof-of-concept carries known risks specific to long-running daemon processes: platform-specific compilation failures, signal handler conflicts, and memory management across an indefinite process lifetime. If these risks are not resolved in a proof-of-concept, W2-5 is deferred. The spike provides the go/no-go, not just a library recommendation.

---

## What to Explore

### 1. Rust FFI Crate Landscape
Evaluate the available Rust interfaces to GGUF/llama.cpp:

- **`llama-cpp-2`**: most actively maintained llama.cpp Rust binding. Evaluate: API stability, ARM + x86 compilation, memory safety story, community size.
- **`candle`** (HuggingFace): pure Rust ML framework with GGUF support. No FFI — no signal handler conflicts by construction. Evaluate: GGUF model loading completeness, performance vs. llama.cpp, ARM support.
- **`mistralrs`**: pure Rust inference engine. Evaluate: GGUF support, performance, deployment story.
- Elimination criteria: must compile for Linux x86_64 + ARM64 without manual llama.cpp source compilation; must be maintained (last commit within 6 months); must support the model sizes needed (1B–7B parameter GGUF).

### 2. Signal Handler Conflicts
- llama.cpp (via C FFI) registers signal handlers for SIGSEGV, SIGBUS, and potentially SIGINT.
- The Unimatrix daemon uses PidGuard with SIGTERM handling and relies on clean process exit for flock release.
- Evaluate: do the llama.cpp signal handlers conflict with the daemon's signal handling? Under what conditions? Is there a documented workaround (signal mask before loading, custom signal handler composition)?
- Pure Rust options (`candle`, `mistralrs`) are exempt from this concern — note explicitly if they pass.

### 3. Memory Management in a Long-Running Process
- A 3B–7B parameter GGUF model loaded persistently uses 2–8GB RAM.
- Evaluate two loading strategies:
  - **Persistent**: model loaded at daemon start, kept in memory indefinitely. Hot inference path (~seconds). Memory committed upfront.
  - **On-demand**: model loaded per inference call, unloaded after. Lower memory floor but cold-start latency per call (model load can take 10–30 seconds for larger models). Inappropriate for interactive use cases.
- For the use cases (context_cycle_review, context_status explanation): are calls synchronous-ish (user waits for output) or truly background? This determines whether persistent or on-demand loading is acceptable.
- Memory fragmentation: GGUF inference allocates and frees large tensors. In a process running for days or weeks, does this cause measurable fragmentation? Is there prior art from other long-running Rust GGUF daemons?

### 4. Rayon Pool Isolation
- GGUF inference must run on a dedicated rayon pool, separate from the ONNX pool (crt-007 / W1-2 established the ONNX pool).
- Validate: can two rayon `ThreadPool` instances coexist in a single process without work-stealing interference or thread starvation?
- Size the GGUF rayon pool: GGUF inference is CPU-bound but serialized (one inference at a time). A pool of 2–4 threads is likely sufficient. Confirm this doesn't starve the ONNX pool under concurrent load.

### 5. Platform Compilation
- Linux x86_64: baseline, should work with any crate.
- Linux ARM64: required. Evaluate: does the chosen crate cross-compile for `aarch64-unknown-linux-gnu`? Are GGUF kernels optimized for ARM NEON/SVE, or x86 AVX2 only?
- macOS (development): needed for dev workflow. Does the crate support macOS natively (Metal backend for inference)?
- Build reproducibility: is the compilation deterministic, or does it pull llama.cpp source at build time (like ONNX `download-binaries`)? Evaluate against the air-gap deployment requirement.

### 6. Model Selection
For the specific use cases:
- `context_cycle_review`: analyze session evidence and produce reasoned recommendations (3–5 sentences per finding). Quality floor: output must be coherent and not obviously wrong.
- `context_status`: convert heuristic threshold results into specific, actionable explanations. Quality floor: factually grounded in the input data.
- `contradiction_explain`: given two conflicting entries, explain why they contradict. Quality floor: accurate characterization without hallucinating.

Evaluate 1B, 3B, and 7B parameter GGUF models (e.g., Llama-3.2-1B, Phi-3.5-mini, Mistral-7B-v0.3) against these use cases. What is the minimum parameter count that meets the quality floor? Smaller models = smaller container, faster inference, lower memory.

---

## Output

1. **Go/no-go recommendation** for W2-5, with explicit reasoning
2. **If go**:
   - FFI crate recommendation with evaluation matrix
   - Model class recommendation (parameter range, specific model family) with quality assessment
   - Loading strategy (persistent vs. on-demand) with memory envelope
   - Compilation strategy for all three target platforms
   - Signal handler conflict resolution (or confirmation that chosen crate avoids the issue)
3. **If no-go**:
   - Which specific risk(s) are blocking
   - What prerequisite condition would change the answer (e.g., `candle` GGUF support matures, llama.cpp signal handling documented)
   - Suggested re-evaluation trigger

---

## Constraints

- Must compile for Linux x86_64 and Linux ARM64 without manual native library compilation
- Air-gap deployable: model file is distributed separately; no internet access at inference time
- SHA-256 hash-pinned model file is a non-negotiable security requirement — any crate that loads arbitrary model URLs without integrity checking is disqualifying
- Must not destabilize the daemon under normal concurrent MCP load (ONNX inference + MCP requests)
- Dedicated rayon pool is non-negotiable — GGUF inference must not share the ONNX pool
