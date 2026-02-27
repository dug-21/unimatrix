# Scope Risk Assessment: infra-001

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | ONNX Runtime version coupling: test-runtime Docker stage must install the exact same ONNX Runtime (1.20.x) the binary was compiled against via ort-sys. Version mismatch causes silent embedding failures or crashes. | High | Medium | Architect should extract ONNX Runtime version from Cargo.lock/build metadata rather than hardcoding. Document version derivation in Dockerfile. |
| SR-02 | Rust toolchain drift: Docker builder uses rust:1.89-bookworm. The workspace requires edition 2024 and patched anndists crate. Future Rust updates may break the build in Docker before they break it locally. | Medium | Medium | Pin exact Rust version in Dockerfile. Copy patches/ directory into builder stage. |
| SR-03 | Python subprocess JSON-RPC fragility: MCP over stdin/stdout requires precise framing (newline-delimited JSON). Interleaved server stderr output, partial writes, or buffering differences between Python subprocess pipes and Rust stdout could cause intermittent parse failures. | High | Medium | Architect should design the client with explicit read buffering, stderr separation, and timeout-with-diagnostics on every read. |
| SR-04 | Embedding model download in Docker build: Pre-downloading the ~90MB all-MiniLM-L6-v2 model requires either network access at build time or bundling it. Model hosting availability affects reproducibility. | Medium | Low | Cache model in a dedicated Docker layer. Document fallback download mechanism. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | 225 test target is ambitious for a single feature: writing, debugging, and maintaining 225 integration tests across 8 suites is significant scope. Risk of incomplete coverage in later suites if earlier suites consume iteration budget. | Medium | Medium | Prioritize suites by risk value: protocol and tools first (foundation), then security and lifecycle. Volume and edge cases last. Spec writer should define a minimum viable test count per suite. |
| SR-06 | Black-box testing cannot observe internal state directly: some acceptance criteria (confidence formula factors, audit log completeness, embedding consistency) require inferring internal state from tool responses. If the server does not expose sufficient detail in its responses, tests become indirect and brittle. | High | Medium | Architect should map each AC to the specific tool response fields that prove it. Identify any ACs that are untestable via MCP protocol alone before committing to them. |
| SR-07 | Test maintenance burden: 225 tests coupled to MCP response formats and server behavior. Any server change that modifies response structure breaks tests across suites. | Medium | High | Design assertion helpers that abstract response structure. Use a response parsing layer so format changes require updating one module, not 225 tests. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | MCP protocol assumptions: the harness assumes specific JSON-RPC framing, initialize/shutdown lifecycle, and tools/call semantics. If rmcp SDK changes protocol behavior in future server updates, tests break silently or noisily. | Medium | Low | Pin expected protocol version in client. Validate server capabilities during initialize before running tool tests. |
| SR-09 | Server startup timing: tests assume ~200ms startup. If embedding model loading or database initialization varies under Docker resource constraints (especially in CI with limited CPU/memory), tests may fail intermittently. | Medium | Medium | Client should use readiness polling (send initialize, retry on timeout) rather than fixed sleep. Configure generous but bounded startup timeout. |
| SR-10 | redb exclusive lock: one process per database file. If test teardown fails to kill server subprocess, subsequent tests on the same temp directory deadlock on database lock. | High | Medium | Implement aggressive cleanup: kill server on fixture teardown with SIGKILL fallback after SIGTERM timeout. Use unique temp directories per test (already planned). |

## Assumptions

1. **Server binary is stable** (SCOPE: "No modifications to the server codebase"): Assumes the current unimatrix-server binary has no bugs that would make tests fail for reasons unrelated to the harness. If existing server bugs surface, they must be triaged as server bugs, not harness failures.

2. **All 9 tools expose sufficient response detail** (SCOPE: AC-07 through AC-13): Assumes tool responses include enough metadata (confidence scores, usage counts, timestamps, chain IDs, audit entries) to validate internal behavior. Some of these fields may only appear in JSON format responses.

3. **Docker build environment has network access** (SCOPE: "Pre-download embedding model"): Build-time network access is required for Rust crate downloads, Python pip installs, and model download. Runtime is offline.

4. **tmpfs is available in CI** (SCOPE: Constraints): Assumes the CI runner supports Docker tmpfs mounts. Some CI environments restrict privileged Docker features.

## Design Recommendations

1. **(SR-03, SR-09)** The MCP client is the critical path component. Architect should treat it as the primary design focus: robust framing, timeout handling, readiness detection, and diagnostic capture on failure.

2. **(SR-06)** Before finalizing the specification, verify that each acceptance criterion can be validated through existing MCP tool response fields. Document the verification path per AC.

3. **(SR-07)** Design a response abstraction layer in the harness that isolates tests from raw JSON structure. This is an architectural decision, not a nice-to-have.

4. **(SR-10)** Server lifecycle management in fixtures must be defensive: PID tracking, SIGTERM with timeout, SIGKILL fallback, temp directory cleanup verification.
