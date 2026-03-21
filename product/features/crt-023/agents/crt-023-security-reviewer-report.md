# Security Review: crt-023-security-reviewer

## Risk Level: medium

## Summary

crt-023 adds an ONNX NLI cross-encoder for search re-ranking and post-store contradiction detection. The implementation is architecturally sound and explicitly addresses the highest-severity security risks: SHA-256 model integrity pinning, per-side input truncation before ONNX inference, numerically stable softmax, `serde_json` metadata serialization, and parameterized SQL throughout. Three findings are present. Finding 1 (non-blocking, medium): `verify_sha256` in `nli_handle.rs` reads the entire model file into RAM in a single allocation, inconsistent with the chunked approach used in `compute_file_sha256` in `main.rs`. Finding 2 (non-blocking, medium): `nli_model_path` accepts arbitrary filesystem paths without canonicalization or absolute-path enforcement. Finding 3 (non-blocking, low): README.md documents `nli_enabled` as "default: true" while the code default is `false` and `NliConfig::default()` sets `nli_enabled: true` — a three-way inconsistency that will confuse operators about whether hash pinning is needed out of the box. No blocking findings; the change is safe to merge with the findings tracked as follow-up issues.

---

## Findings

### Finding 1: `verify_sha256` loads entire model file into memory
- **Severity**: medium
- **Location**: `crates/unimatrix-server/src/infra/nli_handle.rs:554` (`verify_sha256`)
- **Description**: `verify_sha256` calls `std::fs::read(&onnx_file)`, which allocates the full file contents into a single heap `Vec<u8>` before hashing. The primary NLI models are 50–540 MB. The `model-download` CLI's own `compute_file_sha256` in `main.rs` correctly uses a 64 KB chunked `BufReader` to avoid this allocation. The inconsistency means that on a constrained deployment the hash verification step at startup could cause a significant memory spike. The load task runs inside `tokio::task::spawn_blocking`, so a panic from an OOM would result in a `JoinError` caught as `NliState::Failed` — no crash — but the OOM spike itself could affect other concurrent allocations in the process.
- **Recommendation**: Replace `std::fs::read` in `verify_sha256` with the same chunked BufReader pattern used in `compute_file_sha256`. The function is private and only called once at startup; the refactor is low-risk and low-effort.
- **Blocking**: no

### Finding 2: `nli_model_path` accepts arbitrary filesystem paths without canonicalization
- **Severity**: medium
- **Location**: `crates/unimatrix-server/src/infra/nli_handle.rs` (`resolve_model_dir`); `crates/unimatrix-server/src/infra/config.rs` (`InferenceConfig::validate()`)
- **Description**: The config field `nli_model_path: Option<PathBuf>` accepts any path written to the config file, including relative paths, `..`-traversal sequences, and paths to symlinks. `validate()` performs no canonicalization, prefix check, or absolute-path enforcement on this field. The embedding model path handling has the same property (consistent with existing attack surface), but crt-023 adds a new field. In practice, exploiting this requires write access to the config file, which grants full operator control. However, `resolve_model_dir` also loads `tokenizer.json` from the same directory as the ONNX file — an attacker who can redirect this path can load a crafted tokenizer, producing garbage NLI scores silently.
- **Recommendation**: Add an `is_absolute()` check in `validate()` for `nli_model_path` when `Some`, rejecting relative paths. This matches the security posture of other operator-supplied file paths in the codebase.
- **Blocking**: no

### Finding 3: Three-way inconsistency in `nli_enabled` default
- **Severity**: low
- **Location**: `README.md:263` ("default: true"), `crates/unimatrix-server/src/infra/config.rs:307` (`nli_enabled: false`), `crates/unimatrix-server/src/infra/nli_handle.rs:51` (`NliConfig::default()` sets `nli_enabled: true`)
- **Description**: Three sources disagree on the default value of `nli_enabled`:
  1. `README.md` comment says "default: true" and shows `nli_enabled = true` in the example config.
  2. `InferenceConfig::default()` sets `nli_enabled: false` (what the server actually uses at startup if no config is present).
  3. `NliConfig::default()` sets `nli_enabled: true` (used in test helpers, not the startup wiring path).
  The actual production runtime default is `false` (from `InferenceConfig`). The README is wrong. This misleads operators who read the README into thinking NLI is active by default — they may not run `unimatrix model-download --nli` and may not set `nli_model_sha256`, thinking it is set automatically. The security implication is operators who believe NLI is active and expect hash pinning to be enforced will not receive hash verification in the actual default-off state.
- **Recommendation**: Update `README.md` to say "default: false" and change the example to `nli_enabled = false`. Optionally update `NliConfig::default()` to also set `nli_enabled: false` for consistency with `InferenceConfig`, or add a doc comment explaining why it differs (test helper semantics).
- **Blocking**: no

---

## OWASP Assessment

| OWASP Category | Status | Notes |
|---|---|---|
| A03: Injection | PASS | SQL uses parameterized queries throughout; `GRAPH_EDGES` metadata written via `serde_json::json!()` macro (not string concatenation); NLI pair construction uses user content but NLI models are classifiers, not generators — no prompt injection risk |
| A05: Security Misconfiguration | PARTIAL | `nli_model_sha256=None` emits `warn!` log explicitly; `nli_enabled` default confusion (Finding 3) means operators may misread documentation |
| A08: Software and Data Integrity | PARTIAL | SHA-256 pinning implemented and enforced in `nli_handle.rs`; `nli_model_sha256=None` skips verification with a warning per ADR-003; `verify_sha256` reads full file into memory (Finding 1) |
| A01: Broken Access Control | PASS | NLI detection is server-internal; no new MCP tool parameters expose the NLI path; edge writes are gated on `write_pool_server()` with existing access control |
| A09: Security Logging and Monitoring | PASS | Hash mismatch emits `tracing::error!` containing "security" and "hash mismatch" (AC-06 verified in code); mutex poisoning emits `error!`; all NLI degradation events logged |
| Path Traversal | PARTIAL | `nli_model_path` is operator-supplied and not canonicalized (Finding 2); attack requires config file write access |
| Deserialization | PASS | No new deserialization of untrusted data; ONNX model is hash-verified before session construction |
| Input Validation | PASS | Per-side 2000-char truncation enforced inside `score_batch` BEFORE mutex acquisition (confirmed in `cross_encoder.rs`); token-level 512-token truncation via `TruncationParams`; `score_batch(&[])` returns `Ok(vec![])` not an ORT error |

---

## NLI-Specific Security Checks

| Check | Status | Notes |
|---|---|---|
| SHA-256 model pinning | PASS | Implemented in `nli_handle.rs` Step 3; mismatch → `Failed` state + security log |
| Input truncation enforcement | PASS | `truncate_input` called inside `score_batch` BEFORE mutex acquisition; 2000-char + 512-token double enforcement; UTF-8 char boundary respected |
| ONNX session panic containment | PASS | `spawn_blocking` JoinError caught → `NliState::Failed`; mutex poisoning detected at `get_provider()` via `NliProvider::is_session_healthy()` |
| Numerically stable softmax | PASS | Max-subtraction implemented; degenerate sum guard returns uniform distribution instead of panic; confirmed by test coverage |
| Metadata injection | PASS | `serde_json::json!()` macro for metadata; scores are f32 from ONNX output (not user-controlled strings) |
| NLI auto-quarantine cascade (R-09 cap) | PASS | `write_edges_with_cap` counts both Supports AND Contradicts combined (confirmed in code and comments) |
| Circuit breaker correctness | PASS | Cap applied at the start of each iteration; Contradicts skipped for current pair when cap is hit after Supports write — correct behavior |
| Model file integrity on partial download | PASS | `resolve_model_dir` checks `meta.len() > 0`; session construction failure caught by `JoinError` path |
| Hash verification memory (CLI) | PASS | `compute_file_sha256` in `main.rs` uses 64 KB chunked I/O |
| Hash verification memory (server load) | FINDING | `verify_sha256` in `nli_handle.rs` uses single-allocation `std::fs::read` (Finding 1) |
| `nli_enabled` default consistency | FINDING | README and `NliConfig::default()` say `true`; `InferenceConfig::default()` is `false` (Finding 3) |

---

## Blast Radius Assessment

**Worst case if `verify_sha256` full-read causes OOM** (Finding 1): The `spawn_blocking` task panics; `JoinError` is caught; `NliServiceHandle` transitions to `Failed` with `attempts=1`; retry sequence starts with 10s backoff; server continues on cosine fallback. MCP server remains operational. No data corruption, no privilege escalation. Worst case is a temporary memory spike during startup on a constrained host.

**Worst case if `nli_model_path` redirects to a crafted path** (Finding 2): Requires config file write access (operator-level, grants full server control anyway). Attacker can redirect to an arbitrary ONNX file and a crafted `tokenizer.json`, producing adversarial NLI scores that write false `Contradicts` edges or false re-rankings. The `max_contradicts_per_tick` circuit breaker limits per-call flooding. Not a privilege escalation.

**Worst case from `nli_enabled` default confusion** (Finding 3): Operator reads README, believes NLI is enabled by default, does not configure `nli_model_sha256`. At startup, `nli_enabled=false` by `InferenceConfig::default()`, so NLI never loads — the operator never receives hash pinning benefits (or failures). This is a silent degradation (cosine-only search) rather than a security failure, but an operator who explicitly enables NLI and then doesn't set a hash is unprotected from model substitution attacks.

**Worst case for fire-and-forget edge writing**: A single `context_store` call with adversarial content maximizes entailment scores against 10 neighbors, writing up to `max_edges_per_call` edges. The circuit breaker is correctly implemented across both edge types. Auto-quarantine threshold cross-field invariant (`nli_auto_quarantine_threshold > nli_contradiction_threshold`) is enforced at startup.

---

## Regression Risk

**Low.** The NLI path is additive and opt-in (despite README documentation suggesting opt-out):

- `nli_enabled=false` by code default; all NLI code paths are conditional on `get_provider()` returning `Ok`. If the model is absent, all paths fall back to existing cosine-ranked search.
- `SearchService` gains new fields but `nli_enabled=false` leaves the `rerank_score` sort path unchanged.
- `StoreService` fire-and-forget spawn is non-blocking and non-failure-propagating to the MCP response.
- Background tick bootstrap promotion is gated on a COUNTERS marker; zero-rows case sets the marker immediately (AC-12a); no regression on clean databases.
- No schema migration; `GRAPH_EDGES` used as-is with `INSERT OR IGNORE`.
- Existing `InferenceConfig` struct construction in tests updated with `..InferenceConfig::default()` pattern cleanly.
- Pool floor raise (`nli_enabled=true` raises from 4 to 6) is moot when `nli_enabled=false` (code default).

---

## Dependency Safety

| Dependency | Status | Notes |
|---|---|---|
| `sha2 = "0.10"` | PASS | Well-audited RustCrypto crate; no known CVEs at 0.10.x |
| `ort = "=2.0.0-rc.9"` | PASS | Already present; no new version introduced |
| `serde_json = "1"` | PASS | Already present |
| `tempfile = "3"` | PASS | dev-dependency only |
| `hf-hub = "0.4"` | PASS | Already present in `unimatrix-embed`; no new introduction |
| No new production crates | PASS | All dependencies were already in the workspace |

---

## Secrets Audit

No hardcoded credentials, API keys, or tokens found in the diff. `nli_model_sha256` stores a SHA-256 hash (not a secret) and is explicitly operator-configured, not hardcoded.

---

## PR Comments

Posted 1 comment on PR #328 summarizing all three findings.

---

## Knowledge Stewardship

- Attempted: store lesson "Hash verification: full-read vs chunked I/O inconsistency at second call site" via `/uni-store-lesson` — blocked: `uni-security-reviewer` lacks Write capability in Unimatrix. Hand off to coordinator or SM to store under a Write-capable agent.
- Lesson summary for hand-off: when adding a second SHA-256 file-hashing call site, audit existing call sites for I/O pattern consistency and enforce chunked reads for any file that could exceed ~10 MB. Topic: `security`, category: `lesson-learned`, tags: `[security, model-loading, sha256, io, minor]`.
