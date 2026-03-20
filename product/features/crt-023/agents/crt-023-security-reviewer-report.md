# Security Review: crt-023-security-reviewer

## Risk Level: medium

## Summary

crt-023 adds an ONNX NLI cross-encoder for search re-ranking and post-store contradiction detection. The implementation is architecturally sound and explicitly addresses the highest-severity security risks: SHA-256 model integrity pinning, per-side input truncation before ONNX inference, numerically stable softmax, serde_json metadata serialization, and parameterized SQL throughout. Two medium findings require attention before merge: the `verify_sha256` function reads the entire ~85 MB model into RAM in a single `std::fs::read` call (a potential OOM vector under adversarial or misconfigured paths), and `nli_model_path` accepts arbitrary filesystem paths from the config file with no canonicalization or prefix restriction. No blocking findings are present; both are flagged as non-blocking with recommended remediations.

---

## Findings

### Finding 1: `verify_sha256` loads entire model file into memory
- **Severity**: medium
- **Location**: `crates/unimatrix-server/src/infra/nli_handle.rs` lines 554–558 (`verify_sha256`)
- **Description**: `verify_sha256` calls `std::fs::read(&onnx_file)` which allocates the full file into a single heap `Vec<u8>` before hashing. The primary NLI model is ~85 MB. The `model-download` CLI's own `compute_file_sha256` correctly uses a 64 KB chunked `BufReader` to avoid this allocation — that implementation exists in `main.rs:1251–1270`. The loading path in `nli_handle.rs` uses the naive form. This is inconsistent and, if a misconfigured or adversarially large file is placed at the model path, can cause an OOM spike during startup in memory-constrained environments. The loading task runs inside `spawn_blocking`, so a panic from OOM does result in a `JoinError` caught as `NliState::Failed` — no crash — but the OOM itself could affect other allocations in the process.
- **Recommendation**: Replace `std::fs::read` in `verify_sha256` with the same chunked BufReader pattern used in `compute_file_sha256`. The function is private and only called once at startup; refactoring is low-risk.
- **Blocking**: no

### Finding 2: `nli_model_path` accepts arbitrary filesystem paths without canonicalization
- **Severity**: medium
- **Location**: `crates/unimatrix-server/src/infra/nli_handle.rs` `resolve_model_dir` function; `crates/unimatrix-server/src/infra/config.rs` `InferenceConfig::validate()`
- **Description**: The config field `nli_model_path: Option<PathBuf>` accepts any path the operator writes to the config file (e.g., `../../etc/passwd`, symlinks to sensitive directories). `validate()` performs no canonicalization, prefix check, or absolute-path enforcement on this field. The existing embedding model path handling has the same property, so this is consistent with the existing attack surface — but crt-023 adds a new attack vector (separate field). In practice, the attack requires write access to the config file, which grants full operator control anyway, so the blast radius is limited. The concern is the `nli_model_path` directory is also used to load `tokenizer.json` from the same directory — an attacker with config write access could redirect tokenizer loading to a crafted file.
- **Recommendation**: Add an `is_absolute()` check in `validate()` for `nli_model_path` when `Some`, rejecting relative paths. This brings it to parity with the config file path validation already present for other fields. Canonicalization (resolving symlinks) is a stronger fix but is not required for this risk level.
- **Blocking**: no

### Finding 3: Inconsistent SHA-256 case sensitivity in hash comparison
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/nli_handle.rs` line 567 (`verify_sha256`)
- **Description**: `actual_hex` is produced with `{:x}` (lowercase), and the comparison uses `eq_ignore_ascii_case`. The validation in `config.rs` line 453 requires `sha.chars().all(|c| c.is_ascii_hexdigit())` which accepts uppercase hex characters. If an operator provides an uppercase hash (e.g., copy-pasted from a Windows tool), it passes validation but is compared case-insensitively. The behavior is correct (case-insensitive comparison is fine for hex), but the validation docs say "64-character lowercase hex string" while the validator accepts uppercase. This creates a documentation vs. implementation inconsistency that could lead to operator confusion.
- **Recommendation**: Either update the validation error message to say "case-insensitive hex string" or normalize the hash to lowercase on input in `validate()`. No security risk; purely documentation/UX.
- **Blocking**: no

---

## OWASP Assessment

| OWASP Category | Status | Notes |
|---|---|---|
| A03: Injection | PASS | SQL uses parameterized queries throughout; metadata uses `serde_json::json!()` macro, not string concatenation; NLI pair construction operates on user content but NLI models are classifiers, not generators |
| A05: Security Misconfiguration | PASS | `nli_enabled=true` by default is acceptable; `nli_model_sha256=None` emits `warn!` log explicitly |
| A08: Software and Data Integrity | PARTIAL | SHA-256 pinning implemented and enforced in `nli_handle.rs`; `nli_model_sha256=None` skips verification with a warning (intentional design per ADR-003; operator's responsibility to pin in production) |
| A01: Broken Access Control | PASS | NLI detection is server-internal; no new MCP tool parameters expose the NLI path; edge writes are gated on `write_pool_server()` with existing access control |
| A09: Security Logging and Monitoring | PASS | Hash mismatch emits `tracing::error!` containing "security" and "hash mismatch"; mutex poisoning emits `error!`; all NLI degradation events logged |
| Path Traversal | PARTIAL | `nli_model_path` is operator-supplied and not canonicalized (Finding 2) |
| Deserialization | PASS | No new deserialization of untrusted data; ONNX model is hash-verified before session construction |
| Input Validation | PASS | Per-side 2000-char truncation before tokenization; token-level 512-token truncation via `TruncationParams`; `score_batch(&[])` returns `Ok(vec![])` not an ORT error |

---

## NLI-Specific Security Checks

| Check | Status | Notes |
|---|---|---|
| SHA-256 model pinning | PASS | Implemented in `nli_handle.rs`; mismatch → `Failed` state + security log |
| Input truncation enforcement | PASS | `truncate_input` called inside `score_batch` BEFORE mutex acquisition; 2000-char + 512-token double enforcement |
| ONNX session panic containment | PASS | `spawn_blocking` JoinError caught → `NliState::Failed`; mutex poisoning detected at `get_provider()` via `is_session_healthy()` |
| Numerically stable softmax | PASS | Max-subtraction implemented; degenerate sum guard returns uniform distribution instead of panic |
| Metadata injection | PASS | `serde_json::json!()` macro; no string concatenation; scores are f32 from ONNX output (not user-controlled strings) |
| NLI auto-quarantine cascade | PASS | `max_edges_per_call` cap counts both Supports and Contradicts combined (explicitly noted in comments at line ~370); circuit breaker confirmed operational |
| Model file integrity on partial download | PASS | `resolve_model_dir` checks `meta.len() > 0`; session construction failure caught by `JoinError` path |
| Hash verification memory (CLI) | PASS | `compute_file_sha256` in `main.rs` uses chunked I/O |
| Hash verification memory (server load) | FINDING | `verify_sha256` in `nli_handle.rs` uses full-read (Finding 1) |

---

## Blast Radius Assessment

**Worst case if `verify_sha256` full-read causes OOM** (Finding 1): The `spawn_blocking` task panics; `JoinError` is caught; `NliServiceHandle` transitions to `Failed`; retry sequence starts; server continues on cosine fallback. The MCP server remains operational. No data corruption. No privilege escalation. Worst case is a temporary memory spike on startup in a constrained environment.

**Worst case if `nli_model_path` redirects to a crafted path** (Finding 2): Requires config file write access (operator-level). Attacker can load an arbitrary ONNX file as the NLI model and a crafted `tokenizer.json`. A malicious model could produce adversarial NLI scores, generating false `Contradicts` edges or false re-rankings. The `max_contradicts_per_tick` circuit breaker limits per-call edge flooding. An attacker with config write access already has full server control, so this does not constitute a privilege escalation.

**Worst case for the fire-and-forget edge writing**: A single `context_store` call with adversarial content maximizes entailment scores against 10 neighbors, writing `max_edges_per_call` edges. The circuit breaker is correctly implemented (counts Supports + Contradicts combined). Auto-quarantine threshold requires `nli_auto_quarantine_threshold > nli_contradiction_threshold` cross-field invariant enforced at startup.

---

## Regression Risk

**Low.** The NLI path is additive and opt-out:

- `nli_enabled=true` by default, but the NLI path is always conditional on `get_provider()` succeeding. If the model is absent (common in CI), all code paths fall back to the existing cosine-ranked search pipeline.
- `SearchService` gains a new field `nli_handle` but `nli_enabled=false` (or no model) leaves `results_with_scores` on the existing `rerank_score` sort path unchanged.
- `StoreService` fire-and-forget spawn: the `context_store` MCP response path is unchanged — the spawn is non-blocking and non-failure-propagating.
- `background.rs` bootstrap promotion: gated on `bootstrap_nli_promotion_done` COUNTERS marker; zero-rows case sets the marker immediately (AC-12a); no regression on clean databases.
- Schema: no migration; `GRAPH_EDGES` v13 used as-is with `INSERT OR IGNORE`.
- Existing tests: `InferenceConfig` struct construction in tests updated with `..InferenceConfig::default()` pattern — clean, non-breaking.

The sole regression vector is if the pool floor raise (`nli_enabled=true` default raises to 6 from 4) causes resource contention on machines with fewer than 6 logical cores. The max(4).min(8) formula already sets floor at 4; the new logic sets max(6).min(8) when NLI is enabled. This is benign on any modern deployment.

---

## Dependency Safety

| Dependency | Status | Notes |
|---|---|---|
| `sha2 = "0.10"` | PASS | Well-audited RustCrypto crate; no known CVEs at 0.10.x; version pinned |
| `ort = "=2.0.0-rc.9"` | PASS | Already present; no new version introduced |
| `serde_json = "1"` | PASS | Already present |
| `tempfile = "3"` | PASS | dev-dependency only; not in production binary |
| `hf-hub = "0.4"` | PASS | Already present in `unimatrix-embed`; no new introduction |
| No new crates introduced | PASS | All dependencies were already in the workspace |

---

## Secrets Audit

No hardcoded credentials, API keys, or tokens found in the diff. The `nli_model_sha256` field stores a SHA-256 hash (not a secret) and is explicitly described as operator-configured, not hardcoded.

---

## PR Comments

Posted 1 comment on PR #328 (non-blocking findings summary).

---

## Knowledge Stewardship

Nothing novel to store — the two findings (full-read OOM for hash verification, no path canonicalization on operator config paths) are crt-023-specific and follow known patterns already documented in the codebase. The `compute_file_sha256` chunked approach in `main.rs` vs. `std::fs::read` in `nli_handle.rs` is the kind of copy-paste inconsistency better caught by a lint or convention rule, but it does not rise to a generalizable lesson beyond what already exists.
