# Risk-Based Test Strategy: nan-007 (W1-3 Evaluation Harness)

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `AnalyticsMode::Suppressed` not applied at `EvalServiceLayer` construction — analytics drain task runs against read-only snapshot, accumulating silent failures | High | High | Critical |
| R-02 | `SqlxStore::open()` accidentally called on snapshot database, triggering schema migration and corrupting the snapshot | High | Med | High |
| R-03 | `test-support` feature on `unimatrix-engine` removed or gated in future maintenance, silently compiling out `kendall_tau` from the production binary | High | Med | High |
| R-04 | Framing mismatch: `UnimatrixUdsClient` sends length-prefixed frames instead of newline-delimited JSON, causing MCP connection failure | High | Med | High |
| R-05 | Framing mismatch: `UnimatrixHookClient` uses wrong byte order or field ordering in 4-byte BE length prefix, causing hook IPC deserialization failures | High | Med | High |
| R-06 | Snapshot path canonicalization bypass — symlink from output path to live DB path passes naive path equality check, allowing VACUUM INTO to overwrite live database | High | Low | High |
| R-07 | D1–D4 acceptance blocked because daemon fixture failure in D5/D6 is treated as a single gate — offline and live paths conflated during delivery | Med | Med | High |
| R-08 | P@K dual-mode semantics inverted — `expected` used as soft ground truth for query-log scenarios and `baseline.entry_ids` used as hard labels for hand-authored scenarios | High | Med | High |
| R-09 | `ConfidenceWeights` sum invariant produces raw serde parse failure instead of user-readable error, making profile authoring opaque | Med | High | High |
| R-10 | `EvalServiceLayer::from_profile()` panics on missing inference model path instead of returning `EvalError::ModelNotFound` | Med | Med | Med |
| R-11 | `eval run` is not dispatched pre-tokio per C-10 — block_export_sync wrapper missing, leading to nested runtime panic on some platforms | Med | Med | Med |
| R-12 | `eval report` zero-regression check silently omits regressions when MRR is lower but P@K is equal (or vice versa) — combined-OR check not implemented | Med | Med | Med |
| R-13 | `UnimatrixHookClient` sends a payload > 1 MiB without raising `ValueError` before write, violating AC-14 | Med | Med | Med |
| R-14 | UDS socket path validation missing — client connects to paths > 103 bytes, causing OS-level `ENAMETOOLONG` that surfaces as an opaque error | Med | Low | Med |
| R-15 | Vector index loaded per profile grows memory proportionally to profile count; large snapshots × many profiles exhaust available RAM before eval completes | Med | Low | Med |
| R-16 | Scenario JSONL malformed — `baseline.entry_ids` and `baseline.scores` arrays have mismatched lengths, causing silent metric corruption in eval run | Med | Med | Med |
| R-17 | `eval report` produces report with missing section headers (summary table, notable ranking changes, latency distribution, entry-level analysis, zero-regression check) | Low | Med | Low |
| R-18 | `unimatrix snapshot` against a live daemon's WAL-mode DB, with high concurrent write pressure, produces an inconsistent snapshot due to mid-copy WAL checkpoint | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Analytics suppression not enforced at construction

**Severity**: High
**Likelihood**: High
**Impact**: The analytics drain task writes session, co-access, and query-log records into the snapshot database during eval replay. AC-05 (snapshot SHA-256 unchanged after eval run) fails. Eval results are poisoned by write-induced cache effects. Confidence scores in the live database are polluted by synthetic eval queries.

**Test Scenarios**:
1. Run `eval run` against a snapshot, then compare the snapshot file's SHA-256 before and after — assert unchanged (AC-05 verification).
2. Inspect `EvalServiceLayer` construction: assert `AnalyticsMode::Suppressed` is the concrete variant stored on the layer (unit test on `profile.rs`).
3. Attempt to call `enqueue_analytics` on an `EvalServiceLayer` — assert it is a no-op (no channel send, no drain task spawned).
4. Run `eval run` while tailing SQLite WAL for the snapshot file — assert no WAL entries appear.

**Coverage Requirement**: AC-05 must be verified via SHA-256 comparison (NFR-04). Unit test must confirm no drain task is spawned. At least one integration test must run a full `eval run` and confirm snapshot byte-for-byte integrity.

---

### R-02: `SqlxStore::open()` accidentally called on snapshot

**Severity**: High
**Likelihood**: Med
**Impact**: `SqlxStore::open()` triggers schema migration. Even if the migration is a no-op on a matching schema, the schema version write corrupts the snapshot. On a schema mismatch, the migration could alter the snapshot's table structure, making it unusable for replay.

**Test Scenarios**:
1. Code review assertion: grep for `SqlxStore::open` calls inside `eval/` — assert zero occurrences.
2. Open the snapshot with `sqlx::SqlitePool` + `SqliteConnectOptions::read_only(true)` in a unit test — confirm it succeeds without triggering any migration.
3. After `eval scenarios` and `eval run`, verify `sqlite_master` schema version in the snapshot is unchanged (compare against a reference snapshot opened once before eval).

**Coverage Requirement**: Structural test (grep or Rust `#[deny(unused_imports)]` equivalent) to prevent drift. Integration test confirms snapshot schema is unchanged post-eval.

---

### R-03: `test-support` feature removed from `unimatrix-server/Cargo.toml`

**Severity**: High
**Likelihood**: Med
**Impact**: `kendall_tau()` and ranking helpers become invisible to the eval runner. The binary compiles without them — the compile error would surface only when building the `eval` module. Future engineers may remove the feature flag thinking it is test scaffolding.

**Test Scenarios**:
1. CI build of the `unimatrix` binary (release profile) must include `--features test-support` on `unimatrix-engine` — assert the feature appears in `Cargo.lock` resolution for the eval target.
2. Call `kendall_tau()` from within the `eval/runner.rs` module in a unit test — compile failure here signals the feature was removed.
3. Add a doc-comment on the `unimatrix-engine` dependency line in `Cargo.toml` marking it as "production-safe, required by eval runner" — reviewer friction if removed.

**Coverage Requirement**: At least one unit test in `eval/runner.rs` must call `kendall_tau()` directly. Build must fail explicitly (not silently) if the feature is absent.

---

### R-04: `UnimatrixUdsClient` framing mismatch (length-prefix vs newline-delimited)

**Severity**: High
**Likelihood**: Med
**Impact**: If `UnimatrixUdsClient` emits a 4-byte length prefix, the rmcp `JsonRpcMessageCodec` on the server side reads the first 4 bytes as JSON, fails to parse, and closes the connection. All 12 tool calls fail silently or with opaque socket errors.

**Test Scenarios**:
1. Connect `UnimatrixUdsClient` to a live daemon via `daemon_server` fixture — assert `initialize` handshake succeeds within timeout (AC-10).
2. Capture raw bytes sent by `UnimatrixUdsClient` in a test — assert no 4-byte prefix before the `{` of the JSON object, and assert the message terminates with `\n`.
3. Run all 12 `context_*` methods via `UnimatrixUdsClient` and compare results to `UnimatrixClient` stdio — assert parity (AC-10).

**Coverage Requirement**: Framing verification test (raw byte capture) is required. AC-10 parity test is required. Entry #2582 confirms the framing distinction is a known error surface.

---

### R-05: `UnimatrixHookClient` length-prefix framing error

**Severity**: High
**Likelihood**: Med
**Impact**: If the 4-byte header uses little-endian or the wrong field order, the Rust hook IPC reader `read_frame()` reads the wrong byte count, corrupts the JSON body, and returns a deserialization error. Ping/pong succeeds but session events are silently dropped.

**Test Scenarios**:
1. `UnimatrixHookClient.ping()` against a live daemon — assert `HookResponse.type == "Pong"` within timeout (AC-12).
2. Send a `session_start` followed by `session_stop` — assert session record visible in `context_status` output (AC-13).
3. Construct a frame manually with known length and compare byte-for-byte against the output of `struct.pack('>I', len(payload))` — assert endianness is big-endian.
4. Send a frame with a crafted length that exceeds the body — assert the Rust side returns a structured error rather than hanging.

**Coverage Requirement**: AC-12 and AC-13 verification tests are required. Byte-level framing test is required.

---

### R-06: Snapshot path canonicalization bypass

**Severity**: High
**Likelihood**: Low
**Impact**: If the snapshot command compares the raw `--out` path against the active DB path without canonicalization, a symlink pointing to the live DB passes the check. `VACUUM INTO` then overwrites the live database with an identical copy, but the WAL is destroyed, potentially causing data loss.

**Test Scenarios**:
1. Create a symlink pointing to the active database file; pass the symlink path as `--out`; assert non-zero exit code and error message containing both resolved paths (AC-02).
2. Use a relative path for `--out` that resolves to the active DB; assert the same rejection (AC-02).
3. Pass a path where `canonicalize` fails (parent dir does not exist); assert non-zero exit and descriptive error (NFR-06).

**Coverage Requirement**: All three canonicalization edge cases must be covered. This is a security-class test — must not be skipped.

---

### R-07: Offline/live acceptance paths conflated during delivery

**Severity**: Med
**Likelihood**: Med
**Impact**: If the `daemon_server` pytest fixture fails (port conflict, binary not built, socket path issue), D5/D6 tests fail and block the entire test run. D1–D4 acceptance, which requires no daemon, cannot be verified independently. W1-4 and W2-4 gate evidence is blocked.

**Test Scenarios**:
1. Run only the Group 1 (offline) tests with no daemon running — assert all D1–D4 tests pass (AC-01 through AC-09, AC-15).
2. Run only the Group 2 (live) tests — assert that a `daemon_server` fixture failure causes only D5/D6 tests to fail, not D1–D4.
3. Verify pytest marks or test file separation correctly partitions offline and live test suites.

**Coverage Requirement**: Test suite must be structured so `pytest product/test/infra-001/tests/test_eval_offline.py` (or equivalent) passes without a daemon.

---

### R-08: P@K dual-mode semantics inverted

**Severity**: High
**Likelihood**: Med
**Impact**: Query-log scenarios use `expected` (null) instead of `baseline.entry_ids`, computing P@K as 0 for all scenarios. Hand-authored scenarios use `baseline.entry_ids` (the old results) instead of `expected` (the authoritative labels), producing P@K against the wrong ground truth. Eval reports are meaningless — regressions are invisible.

**Test Scenarios**:
1. Run `eval run` with a hand-authored scenario where `expected = [id1, id2]` — assert P@K is computed relative to `expected`, not `baseline` (AC-07).
2. Run `eval run` with a query-log scenario where `expected = null` and `baseline.entry_ids = [id3, id4]` — assert P@K is computed relative to `baseline.entry_ids` (AC-07).
3. Run `eval run` with a hand-authored scenario and a profile that returns a known top result — assert P@K@1 = 1.0 when that result is in `expected`.

**Coverage Requirement**: Both branches of the dual-mode P@K dispatch must have at least one dedicated test. AC-07 verification is required.

---

### R-09: `ConfidenceWeights` validation produces opaque serde error

**Severity**: Med
**Likelihood**: High
**Impact**: A candidate profile with weights summing to 0.91 fails at TOML parse time with a raw serde error, not a structured message. The user cannot determine which field is wrong or what the correct sum should be. All eval runs with weight-tuning profiles fail with an unintelligible error (SR-08).

**Test Scenarios**:
1. Load a profile TOML where `[confidence]` weights sum to 0.91 — assert `EvalError::ConfigInvariant` is returned (not serde error) and the message contains the expected sum (0.92) and the actual sum (0.91) (FR-18).
2. Load a profile TOML with one weight field missing — assert the same structured error with actionable message.
3. Load a valid profile TOML with weights summing exactly to 0.92 — assert construction succeeds.

**Coverage Requirement**: Error message content must be asserted, not just the error variant. Boundary cases: sum = 0.92 ± 1e-9 (pass), sum = 0.92 ± 2e-9 (fail).

---

### R-10: Panic on missing inference model path

**Severity**: Med
**Likelihood**: Med
**Impact**: A profile TOML specifying `[inference] nli_model = "/nonexistent/model.onnx"` causes `EvalServiceLayer::from_profile()` to panic at construction — or worse, panic at the first inference call — killing the eval run without a structured error and no result files written (SR-09).

**Test Scenarios**:
1. Call `EvalServiceLayer::from_profile()` with a profile specifying a non-existent model path — assert `Err(EvalError::ModelNotFound)` is returned, not a panic (FR-23).
2. Call with an unreadable model file (exists, permissions denied) — assert same structured error variant.
3. Call with a valid profile (no inference overrides) — assert construction succeeds.

**Coverage Requirement**: At minimum one test per error variant. No `unwrap()` or `expect()` in the model path validation code path.

---

### R-11: Nested runtime panic from missing `block_export_sync` wrapper

**Severity**: Med
**Likelihood**: Med
**Impact**: If `eval run` or `eval scenarios` initiates a tokio runtime inside an already-running tokio context (e.g., called from an async test), the `block_on` panics with "Cannot start a runtime from within a Tokio runtime." The error is platform-specific and may not reproduce on all CI configurations.

**Test Scenarios**:
1. Invoke `run_eval_command(EvalCommand::Run, ...)` from a `#[tokio::test]` context via `tokio::task::block_in_place` — assert it does not panic.
2. Invoke `run_scenarios` from the sync dispatch path in `main()` — confirm no runtime creation occurs outside `block_export_sync`.
3. Integration test that calls `unimatrix eval scenarios` as a subprocess — assert exit code 0 on a valid snapshot.

**Coverage Requirement**: The async bridge path must be exercised in at least one integration test.

---

### R-12: Zero-regression check misses partial regressions

**Severity**: Med
**Likelihood**: Med
**Impact**: The zero-regression check in `eval report` only fires when both MRR and P@K are lower — a scenario where MRR drops but P@K is unchanged (or vice versa) is silently omitted from the list. Human reviewer sees an empty regression list and ships a change that degraded MRR for a significant query class (AC-09).

**Test Scenarios**:
1. Build a result set where candidate has `mrr = 0.4`, baseline has `mrr = 0.5`, but `p_at_k` is equal — assert the scenario appears in the zero-regression list.
2. Build a result set where candidate has `p_at_k` lower but `mrr` equal — assert the scenario appears.
3. Build a result set with no regressions — assert the zero-regression section contains the explicit empty-list indicator (AC-09).

**Coverage Requirement**: The OR semantics of the regression check must be explicitly tested. Both metric branches must have failing cases.

---

### R-13: `UnimatrixHookClient` size guard fires after send, not before

**Severity**: Med
**Likelihood**: Med
**Impact**: A payload just over 1 MiB is serialized, sent partially, then the guard fires — leaving the socket in a partial-write state that causes subsequent calls to fail with opaque framing errors. AC-14 requires the `ValueError` before any bytes are sent.

**Test Scenarios**:
1. Construct a `pre_tool_use` payload of exactly 1,048,577 bytes — assert `ValueError` raised before any socket write (AC-14).
2. Mock the socket to confirm zero bytes were written before the exception (or verify socket send is not called).
3. After a rejected oversized payload, confirm the client is still usable for a normal-sized `ping()`.

**Coverage Requirement**: AC-14 test must be present. Pre-send guard placement is a behavioral contract.

---

### R-14: UDS socket path exceeds 103-byte OS limit

**Severity**: Med
**Likelihood**: Low
**Impact**: `AF_UNIX` on Linux has a 108-byte `sun_path` limit (103 bytes usable per C-08). If `UnimatrixUdsClient` does not validate before calling `connect()`, the Python `socket.connect()` call raises `OSError: [Errno 36] File name too long` — an opaque OS error that gives no hint about the 103-byte constraint.

**Test Scenarios**:
1. Pass a socket path of exactly 104 bytes to `UnimatrixUdsClient.__init__` — assert a descriptive `ValueError` (not an `OSError`) is raised before `connect()` is called (FR-31).
2. Pass a path of exactly 103 bytes — assert no error raised.
3. Pass a path of 1 byte — assert no error.

**Coverage Requirement**: Boundary condition at 103/104 bytes must be tested.

---

### R-15: Vector index memory exhaustion for multi-profile eval

**Severity**: Med
**Likelihood**: Low
**Impact**: Each `EvalServiceLayer` loads a full HNSW index. On a 50,000-entry snapshot with 3 candidate profiles, three indexes are resident simultaneously. If this exceeds available RAM, the process is OOM-killed and no result files are written.

**Test Scenarios**:
1. Run `eval run` with 2 profiles against a representative snapshot — assert completion without OOM (NFR-03).
2. Document the per-index memory estimate in the eval `--help` text and assert the text is present.
3. Verify the architecture decision (one index per profile, not shared) is consistent with the CLI help documentation.

**Coverage Requirement**: Functional test with 2 profiles required. OOM is not reliably testable in unit tests but NFR-03 sets the measurable threshold (2 profiles, 50k entries, 8 GB RAM).

---

### R-16: Mismatched `baseline.entry_ids` and `baseline.scores` lengths

**Severity**: Med
**Likelihood**: Med
**Impact**: If `eval scenarios` emits a JSONL scenario where `entry_ids` has N items but `scores` has N-1 items (due to a join producing a partial row), `eval run` computes MRR and P@K against misaligned data, producing metric values that are numerically valid but semantically wrong. No error is raised.

**Test Scenarios**:
1. Validate JSONL output from `eval scenarios` — assert `len(baseline.entry_ids) == len(baseline.scores)` for every scenario line (AC-03).
2. Introduce a controlled scenario where a `query_log` row has `result_entry_ids` and `similarity_scores` with mismatched JSON array lengths — assert `eval scenarios` either normalizes or rejects the row with an error.
3. Run `eval run` with a mismatched scenario — assert structured error rather than silent metric corruption.

**Coverage Requirement**: Length parity validation must be tested in the `eval scenarios` output parser and the `eval run` scenario loader.

---

## Integration Risks

### Offline Path (D1–D4): read-only enforcement chain

The read-only guarantee passes through three independent layers: (1) `SqliteConnectOptions::read_only(true)` on the pool, (2) `AnalyticsMode::Suppressed` suppressing the drain task, and (3) `SqlxStore::open()` never called on the snapshot. All three must hold simultaneously. A regression in any one layer can corrupt the snapshot. Tests must verify all three independently and the combined effect via SHA-256 comparison (NFR-04).

### Live Path (D5–D6): two socket protocols on one daemon

The daemon exposes two distinct sockets with incompatible framing protocols. `UnimatrixUdsClient` (newline-delimited JSON) and `UnimatrixHookClient` (4-byte BE + JSON) must never be swapped. Integration tests must verify each client connects to the correct socket path via `ProjectPaths.mcp_socket_path` and `ProjectPaths.socket_path` respectively. A test that accidentally sends MCP framing to the hook socket produces a cascade of deserialization errors that are hard to diagnose.

### Analytics suppression boundary: eval vs. production

`EvalServiceLayer` must not leak analytics calls into the production `ServiceLayer` path. Tests should confirm that `enqueue_analytics` calls made during `eval run` do not affect the live database's `query_log`, `co_access`, or `sessions` tables. This is most directly verified by running `eval run` against a snapshot and then querying `context_status` on the live database to confirm no new entries appear.

### `block_export_sync` re-entrance boundary

`eval run` and `eval scenarios` use `block_export_sync` to bridge sync dispatch to async sqlx. If either subcommand is called from within an existing tokio runtime (e.g., a test harness), nested `block_on` panics. The test suite must structure integration tests for `eval run` as subprocess invocations or use `block_in_place` rather than nesting runtimes.

---

## Edge Cases

- **Empty snapshot**: `query_log` table has zero rows. `eval scenarios` must emit an empty JSONL file (not an error). `eval run` on an empty scenarios file must emit an empty results directory (not an error). `eval report` on an empty results directory must produce a report with the zero-regression check empty-list indicator.
- **Scenarios file with a single entry**: Kendall tau for a single-element list is undefined (or 1.0 by convention). The implementation must handle this without a divide-by-zero or NaN result.
- **Profile TOML with only `[profile]` section and no overrides**: This is the baseline profile. Construction must succeed with compiled defaults applied.
- **Snapshot taken mid-WAL-checkpoint**: VACUUM INTO on a WAL-mode database reads a consistent point-in-time snapshot. If a checkpoint runs concurrently, WAL isolation guarantees the snapshot reflects state before the checkpoint. This is a SQLite guarantee; the test scenario is documented, not unit-tested.
- **`--out` path for snapshot does not exist**: Parent directory missing. The subcommand must fail with a descriptive error before executing `VACUUM INTO`, not after a partial write.
- **Unicode characters in query text**: Scenario JSONL must encode multi-byte Unicode query strings correctly. `json.dumps` in Python defaults to ASCII escaping — `UnimatrixHookClient` must emit proper UTF-8 JSON.
- **Profile name collision**: Two profile TOMLs with the same `[profile] name` field. `eval run` must fail with a structured error naming the duplicate, not silently overwrite one profile's results.
- **`--k 0` or negative K**: P@K with K=0 is meaningless. `eval run` must validate `--k >= 1` and return a user-readable error.

---

## Security Risks

### Snapshot path traversal (R-06 detailed)

**Untrusted input**: `--out <path>` argument accepted from the CLI.
**Attack vector**: A symlink or path with `..` components can resolve to the active database path.
**Damage**: `VACUUM INTO` overwrites the live database, destroying the WAL and potentially corrupting ongoing daemon writes.
**Blast radius**: Full database loss if the daemon is writing during the overwrite.
**Mitigation**: `std::fs::canonicalize` on both paths before comparison (NFR-06). `canonicalize` resolves all symlinks and `..` components. Failure of `canonicalize` on either path must cause the command to abort.

### Snapshot content sensitivity

**Untrusted context**: The snapshot contains all database tables including `agent_id`, `session_id`, `query_log` text, and `audit_log`. Sharing a snapshot outside the development environment exposes operational metadata.
**Mitigation**: CLI help text must warn that the snapshot contains all database content (NFR-07). No automated scrubbing is in scope; the risk is documented, not mitigated programmatically.

### Hook client oversized payload

**Untrusted input**: `UnimatrixHookClient` callers construct arbitrary payloads.
**Attack vector**: A caller supplies a >1 MiB payload without the client enforcing `MAX_PAYLOAD_SIZE`.
**Damage**: The Rust hook IPC handler reads a 4-byte length, allocates a buffer of that size, and reads that many bytes. A crafted oversized payload can cause the server to allocate up to 1 GiB before the `MAX_PAYLOAD_SIZE` server-side guard fires.
**Blast radius**: OOM on the daemon process if the server-side guard is also absent.
**Mitigation**: Client-side guard raises `ValueError` before any send (AC-14). Server-side `MAX_PAYLOAD_SIZE` is also enforced in `unimatrix_engine::wire`. Both layers must be tested independently.

### Profile TOML injection

**Untrusted input**: Profile TOML files are accepted from the filesystem at eval run time.
**Attack vector**: A profile TOML could set `inference.model_path` to an arbitrary file path, causing the eval runner to attempt to load an attacker-controlled binary as an ONNX model.
**Damage**: Arbitrary code execution via a malicious ONNX model if ONNX runtime does not sandbox model loading.
**Blast radius**: Full process compromise.
**Mitigation**: Model path validation at `EvalServiceLayer` construction (FR-23, R-10) confirms the file exists and is readable. The ONNX runtime's own model validation is the primary security boundary. Profile TOMLs are treated as developer-trusted artifacts, not user-supplied data — no sandbox is in scope.

---

## Failure Modes

| Component | Failure Mode | Expected Behavior |
|-----------|-------------|-------------------|
| `unimatrix snapshot` | `--out` path resolves to live DB | Non-zero exit, error message naming both paths |
| `unimatrix snapshot` | Parent directory missing | Non-zero exit before VACUUM INTO executes |
| `unimatrix snapshot` | `canonicalize` fails on source path | Non-zero exit, descriptive error |
| `eval scenarios` | Snapshot DB not readable | Non-zero exit, OS error with path |
| `eval scenarios` | `query_log` is empty | Exit 0, empty JSONL file |
| `eval scenarios` | Invalid `--retrieval-mode` value | Clap rejects at parse time with help text |
| `eval run` | Profile TOML fails `ConfidenceWeights` invariant | `EvalError::ConfigInvariant` with expected/actual sums |
| `eval run` | Profile TOML model path missing | `EvalError::ModelNotFound` before any scenario executes |
| `eval run` | Scenarios file is empty | Exit 0, empty results directory |
| `eval run` | Profile name collision | Structured error naming the duplicate before any replay |
| `eval run` | Snapshot pool fails to open (permissions) | Non-zero exit, OS error |
| `eval report` | Results directory empty | Exit 0, report with empty-list indicators in all sections |
| `eval report` | Malformed result JSON file | Skip file with warning, continue with valid files |
| `UnimatrixUdsClient` | Socket path > 103 bytes | `ValueError` before `connect()` |
| `UnimatrixUdsClient` | Socket file does not exist | `ConnectionError` with socket path in message |
| `UnimatrixUdsClient` | Daemon closes connection mid-session | `IOError` propagated to caller; context manager `__exit__` cleans up |
| `UnimatrixHookClient` | Payload > 1 MiB | `ValueError` before any send |
| `UnimatrixHookClient` | Daemon not listening on hook socket | `ConnectionError` with socket path |
| `UnimatrixHookClient` | Partial read on response (daemon killed mid-response) | `IOError` propagated to caller |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 | R-03 | rmcp already pinned to `=0.16.0`. Integration smoke test for UDS `serve()` path covers compile-time regression. ADR-003 complements by ensuring `test-support` feature is explicitly declared. |
| SR-02 | R-18 | ADR-001 documents WAL-mode isolation guarantees: VACUUM INTO reads a consistent snapshot even with concurrent daemon writes. Risk accepted; operating mode documented in CLI help. |
| SR-03 | R-15 | Architecture documents one VectorIndex per EvalServiceLayer per profile; memory limit noted in CLI help text (NFR-03 sets measurable threshold: 2 profiles, 50k entries, 8 GB RAM). |
| SR-04 | R-07 | SPECIFICATION groups acceptance criteria into Group 1 (offline, D1–D4) and Group 2 (live, D5–D6). Test files must be split accordingly. D5/D6 fixture failure must not block D1–D4. |
| SR-05 | — | Resolved in architecture: `ProjectPaths.socket_path` is the hook IPC socket, `ProjectPaths.mcp_socket_path` is the MCP socket. No new `ProjectPaths` field needed. No residual risk. |
| SR-06 | — | C-07 constraint forbids CI gate logic in `eval report`. Non-goal is stated in spec and enforced structurally (FR-29). No residual risk requiring test coverage. |
| SR-07 | R-01 | ADR-002 mandates `AnalyticsMode::Suppressed` at `EvalServiceLayer` construction. SHA-256 snapshot integrity test (AC-05/NFR-04) is the verification target. |
| SR-08 | R-09 | Architecture specifies `EvalError::ConfigInvariant(String)` with user-readable message naming expected/actual sums. Test must assert message content, not just variant. |
| SR-09 | R-10 | Architecture specifies `EvalError::ModelNotFound` returned at `from_profile()` construction time. No panics permitted in the profile loading code path. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 4 scenarios: SHA-256 integrity, construction unit test, no-op assert, WAL check |
| High | 7 (R-02 through R-08) | 3–4 scenarios each; AC-02, AC-05, AC-07, AC-10, AC-12, AC-13 all required |
| Med | 8 (R-09 through R-16) | 2–3 scenarios each; AC-03, AC-09, AC-14, FR-18, FR-23, FR-31 verification |
| Low | 2 (R-17, R-18) | 1–2 scenarios each; section-header assertion for report; documented, not unit-tested for WAL |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection eval harness" — entries #1203, #1204, #2577 confirm that missing boundary tests and test/pseudocode cross-reference are recurring gate failure patterns. R-12 (partial regression check) and R-16 (array length mismatch) are directly informed by this history.
- Queried: `/uni-knowledge-search` for "risk pattern read-only analytics suppression SQLite" — entry #2125 (analytics drain unsuitable for immediate-visibility reads) and entry #2130 (SQLITE_BUSY_SNAPSHOT with fire-and-forget writes) both support the critical priority assigned to R-01. Entry #2582 (MCP UDS framing confirmed as newline-delimited JSON) is evidence for R-04/R-05 severity.
- Queried: `/uni-knowledge-search` for "migration read-only enforcement integration test" — entry #2060 (migration connection sequencing) reinforces R-02 (accidental `SqlxStore::open()` on snapshot).
- Stored: nothing novel to store — the two-socket / two-framing risk pattern is already captured in entry #2582; the analytics suppression pattern is specific to nan-007's design. Will revisit after delivery for cross-feature promotion.
