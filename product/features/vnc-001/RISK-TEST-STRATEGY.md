# Risk-Based Test Strategy: vnc-001

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | MCP initialize handshake fails or returns incomplete ServerInfo | High | Medium | Critical |
| R-02 | Project root detection fails in non-git directories or symlinked paths | Medium | High | High |
| R-03 | Project hash collision or non-determinism across sessions | High | Low | High |
| R-04 | AGENT_REGISTRY table creation breaks existing Store::open() for downstream crates | High | Medium | Critical |
| R-05 | Default agent bootstrap runs on every open (not just first run) | Medium | Medium | High |
| R-06 | Auto-enrollment creates agents with incorrect capabilities | High | Medium | Critical |
| R-07 | Audit log ID generation collides or wraps around | Medium | Low | Medium |
| R-08 | Graceful shutdown fails to call compact() due to leaked Arc references | Medium | High | High |
| R-09 | VectorIndex::dump() fails during shutdown, losing vector data | Medium | Medium | High |
| R-10 | Embedding model download failure leaves server in permanent Failed state | Medium | Medium | High |
| R-11 | Tool stubs don't match expected MCP schema (wrong param names/types) | High | Medium | Critical |
| R-12 | Agent identity not threaded through audit log (events lack agent_id) | High | Medium | Critical |
| R-13 | Error responses contain internal details instead of actionable guidance | Medium | Medium | High |
| R-14 | Server panics on malformed MCP input instead of returning error | High | Medium | Critical |
| R-15 | Data directory permissions prevent creation on restricted systems | Medium | Medium | High |
| R-16 | Concurrent tool calls corrupt shared state (registry, audit) | High | Low | High |

## Risk-to-Scenario Mapping

### R-01: MCP Initialize Handshake Failure

**Severity**: High
**Likelihood**: Medium
**Impact**: Server appears dead to the MCP client. No tool calls possible. Agent workflow halts completely.

**Test Scenarios**:
1. Start server with stdio pipes, send valid `initialize` request, verify response contains `serverInfo` with name="unimatrix", non-empty version, non-empty instructions, and tool capabilities.
2. Send `initialize` with minimal valid request body (no clientInfo), verify server responds without error.
3. Verify `get_info()` returns ServerInfo with all required fields populated (unit test).
4. Send `initialized` notification after init, verify server accepts it (lifecycle completeness).
5. Send `ping` request, verify server responds with pong.

**Coverage Requirement**: Full MCP initialization lifecycle tested. ServerInfo validated for all required fields.

### R-02: Project Root Detection Failure

**Severity**: Medium
**Likelihood**: High
**Impact**: Server creates data directory in wrong location. Different sessions for the same project may use different data directories, causing data loss.

**Test Scenarios**:
1. Run detection from a directory 3 levels below `.git/`. Verify it finds the correct root.
2. Run detection from the `.git/` directory's parent. Verify it returns that directory.
3. Run detection from a directory with no `.git/` ancestors. Verify it returns cwd.
4. Run detection from a symlinked directory. Verify the canonical (resolved) path is used, not the symlink path.
5. Run detection from the filesystem root. Verify it returns the root without infinite loop.
6. Run detection with `--project-dir` override. Verify the override takes precedence.

**Coverage Requirement**: Detection tested for: nested git project, root-level git project, no-git fallback, symlinks, filesystem root, CLI override.

### R-03: Project Hash Non-Determinism

**Severity**: High
**Likelihood**: Low
**Impact**: Same project produces different hashes across sessions, creating orphaned data directories and losing accumulated knowledge.

**Test Scenarios**:
1. Compute hash for the same canonical path twice. Verify identical results.
2. Compute hash for `/home/user/project` and `/home/user/../user/project`. Verify same hash (after canonicalization).
3. Verify hash is exactly 16 hex characters long.
4. Compute hash for two different paths. Verify different hashes.
5. Verify hash uses SHA-256, not a weaker hash.

**Coverage Requirement**: Determinism verified across calls. Canonicalization verified to prevent path aliasing.

### R-04: AGENT_REGISTRY Table Creation Breaks Existing Store::open()

**Severity**: High
**Likelihood**: Medium
**Impact**: All existing tests fail. Downstream crates that depend on unimatrix-store break. Regression in nxs-001 through nxs-004.

**Test Scenarios**:
1. Open a database created by pre-vnc-001 Store::open(). Verify it opens without error and all 8 original tables are accessible.
2. Open the same database again. Verify AGENT_REGISTRY and AUDIT_LOG tables now exist alongside the original 8.
3. Run the full existing nxs-001 test suite after adding the new table definitions. Verify all tests pass.
4. Verify AGENT_REGISTRY table can store and retrieve an AgentRecord via bincode round-trip.
5. Verify AUDIT_LOG table can store and retrieve an AuditEvent via bincode round-trip.

**Coverage Requirement**: Backward compatibility with existing databases verified. All 10 tables accessible. Existing test suite regression checked.

### R-05: Default Agent Bootstrap Runs on Every Open

**Severity**: Medium
**Likelihood**: Medium
**Impact**: "human" and "system" agents are re-created on every server start, potentially resetting modifications to their trust levels or capabilities made by an admin.

**Test Scenarios**:
1. First run: verify "human" and "system" agents are created.
2. Modify "human" agent's capabilities (add Admin). Restart server. Verify modification persists.
3. Verify bootstrap checks for existing agents before creating (idempotent).
4. Open fresh database, bootstrap, verify exactly 2 agents exist.
5. Open database with existing agents, bootstrap, verify agent count unchanged.

**Coverage Requirement**: Bootstrap idempotency verified. Existing agent preservation verified.

### R-06: Auto-Enrollment Creates Agents with Incorrect Capabilities

**Severity**: High
**Likelihood**: Medium
**Impact**: Unknown agents get Write or Admin capabilities, bypassing the trust hierarchy. Security model undermined from day one.

**Test Scenarios**:
1. Resolve an unknown agent_id. Verify trust_level = Restricted, capabilities = [Read, Search].
2. Resolve "anonymous" (default for missing agent_id). Verify Restricted with [Read, Search].
3. Resolve an agent_id that matches a pre-existing entry ("human"). Verify existing trust level is returned, not Restricted.
4. Verify auto-enrolled agents do NOT have Write or Admin capabilities.
5. Verify `require_capability(unknown_agent, Capability::Write)` returns CapabilityDenied error.
6. Verify `has_capability(unknown_agent, Capability::Read)` returns true.

**Coverage Requirement**: Every TrustLevel's default capabilities verified. Auto-enrollment capability set verified. Negative tests for capability denial.

### R-07: Audit Log ID Collision

**Severity**: Medium
**Likelihood**: Low
**Impact**: Audit events overwrite each other. Forensic trail becomes unreliable.

**Test Scenarios**:
1. Log 100 events rapidly. Verify all 100 have unique, strictly increasing IDs.
2. Verify IDs start from 1 on first use (COUNTERS["next_audit_id"] not set).
3. Log events across two simulated sessions (close and reopen store). Verify IDs continue from where the first session left off.
4. Verify AuditEvent round-trips through bincode (all fields preserved).

**Coverage Requirement**: Monotonicity verified. Cross-session continuity verified. Serialization verified.

### R-08: Graceful Shutdown Fails to Call compact()

**Severity**: Medium
**Likelihood**: High
**Impact**: Database file grows unbounded over time. Not a correctness issue (redb is crash-safe) but a resource issue.

**Test Scenarios**:
1. Create server with known Arc reference count. Drop all clones. Verify `Arc::try_unwrap()` succeeds.
2. Simulate leaked Arc reference (e.g., clone held in a dropped-but-not-joined task). Verify try_unwrap fails gracefully with a log warning, not a panic.
3. Verify compact() is called when try_unwrap succeeds (check file size reduction or at minimum no error).
4. Verify the server exits with code 0 regardless of compact success/failure.
5. Verify VectorIndex::dump() is called BEFORE Store Arc clones are dropped (ordering).

**Coverage Requirement**: Shutdown sequence ordering verified. Graceful degradation on leaked Arcs verified. Exit code verified.

### R-09: VectorIndex::dump() Failure During Shutdown

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Vector index data lost. On next startup, a fresh empty index is created, requiring re-embedding of all entries.

**Test Scenarios**:
1. Insert entries and vectors. Trigger shutdown. Verify dump files exist in vector directory.
2. Make vector directory read-only. Trigger shutdown. Verify dump failure is logged but shutdown continues (compact still runs).
3. Verify that after successful dump + restart, the loaded index contains the same point count.

**Coverage Requirement**: Successful dump verified. Dump failure handling verified (non-fatal). Dump-load round-trip verified.

### R-10: Embedding Model Download Failure

**Severity**: Medium
**Likelihood**: Medium
**Impact**: context_search permanently unavailable for the session. Agent workflows that depend on semantic search fail.

**Test Scenarios**:
1. Start server with model already cached. Verify EmbedServiceHandle transitions to Ready.
2. Start server with invalid model config (non-existent model name). Verify handle transitions to Failed, not panic.
3. With handle in Failed state, call a method that requires embeddings. Verify structured error with code -32004.
4. With handle in Loading state, call a method that requires embeddings. Verify structured error with code -32004.
5. Verify context_lookup and context_get do NOT check embed readiness (they work without embeddings).

**Coverage Requirement**: All three states (Loading, Ready, Failed) tested. Error messages verified. Non-embedding tools verified independent.

### R-11: Tool Schema Mismatch

**Severity**: High
**Likelihood**: Medium
**Impact**: MCP client cannot correctly invoke tools. Parameter names or types don't match what agents expect. Silent data loss from ignored parameters.

**Test Scenarios**:
1. Send `tools/list` request. Verify 4 tools returned with correct names: context_search, context_lookup, context_store, context_get.
2. Verify context_search has required `query` parameter of type string.
3. Verify context_store has required `content`, `topic`, `category` parameters.
4. Verify context_get has required `id` parameter of type integer.
5. Verify all tools have optional `agent_id` parameter.
6. Verify tool descriptions match ASS-007 interface specification wording.
7. Verify tool annotations: context_search and context_lookup have readOnlyHint=true.

**Coverage Requirement**: All 4 tool schemas verified against spec. Required vs optional params checked. Annotations checked.

### R-12: Agent Identity Not Threaded Through Audit

**Severity**: High
**Likelihood**: Medium
**Impact**: Audit log contains empty or default agent_ids. Cannot trace which agent made which request. Security audit trail broken.

**Test Scenarios**:
1. Call tool stub with `agent_id: "uni-architect"`. Read audit log. Verify event has agent_id="uni-architect".
2. Call tool stub without agent_id. Read audit log. Verify event has agent_id="anonymous".
3. Call tool stub with empty string agent_id. Verify defaults to "anonymous".
4. Verify audit event contains correct operation name (e.g., "context_search").
5. Verify audit event contains correct outcome (NotImplemented for stubs).

**Coverage Requirement**: Identity threading verified for explicit, missing, and empty agent_id. Audit event completeness verified.

### R-13: Error Responses Leak Internal Details

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Agents receive raw Rust error messages instead of actionable guidance. Poor agent experience; potential information leakage.

**Test Scenarios**:
1. Trigger entry-not-found error. Verify response contains helpful message (e.g., "Verify the ID from a previous search result") not raw StoreError.
2. Trigger capability-denied error. Verify message names the agent and suggests contacting admin.
3. Trigger embedding-not-ready error. Verify message suggests using context_lookup as alternative.
4. Verify no error response contains Rust type names (e.g., "StoreError::EntryNotFound").
5. Verify all error responses have numeric MCP error codes.

**Coverage Requirement**: Every ServerError variant mapped to user-facing message. No raw error leakage.

### R-14: Server Panic on Malformed MCP Input

**Severity**: High
**Likelihood**: Medium
**Impact**: Server crashes, losing the MCP session. In-progress agent work is lost. User must restart.

**Test Scenarios**:
1. Send tool call with wrong parameter types (string where int expected). Verify error response, not panic.
2. Send tool call with extra unknown parameters. Verify they are ignored, not panic.
3. Send tool call with missing required parameters. Verify error response with -32602 code.
4. Send extremely large parameter values (1MB string). Verify server handles it (possibly with error) without OOM crash.
5. Send tool call for non-existent tool name. Verify error response.

**Coverage Requirement**: Malformed input handling verified for type mismatches, missing params, extra params, oversized params, and unknown tools.

### R-15: Data Directory Permission Issues

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Server cannot start. No data persisted. User gets cryptic filesystem error.

**Test Scenarios**:
1. Start server with writable home directory. Verify data directory created successfully.
2. Verify error message includes the full path when directory creation fails.
3. Verify the server creates parent directories recursively (`~/.unimatrix/` may not exist).

**Coverage Requirement**: Directory creation verified including parent creation. Error messages include paths.

### R-16: Concurrent Tool Call State Corruption

**Severity**: High
**Likelihood**: Low
**Impact**: Registry or audit log has corrupted data. Agent records lost or merged incorrectly.

**Test Scenarios**:
1. Send 10 tool calls in rapid succession (pipeline). Verify all 10 audit events are recorded with correct data.
2. Resolve the same unknown agent_id concurrently from two calls. Verify exactly one enrollment occurs (not two).
3. Verify AgentRegistry operations use redb transactions (single-writer serialization).
4. Verify AuditLog ID generation is atomic (no duplicate IDs under concurrent calls).

**Coverage Requirement**: Concurrent access patterns verified. Transaction isolation relied upon (redb single-writer).

## Integration Risks

### IR-01: unimatrix-store Table Extension Backward Compatibility

Adding AGENT_REGISTRY and AUDIT_LOG to `Store::open()` must not break any existing functionality. The 8 original tables must remain identical. Existing tests in nxs-001 through nxs-004 must pass without modification.

**Scenarios**: Run `cargo test -p unimatrix-store` after adding new table definitions. Run `cargo test -p unimatrix-core` after Store changes.

### IR-02: AsyncEntryStore + StoreAdapter Wiring

The server depends on the full chain: `AsyncEntryStore<StoreAdapter>`. A type mismatch anywhere in this chain (e.g., missing trait bound, wrong Arc wrapping) causes compile failure. Existing nxs-004 tests validate this chain, but vnc-001's construction path may differ.

**Scenarios**: Compile the server crate. Exercise the full async chain in an integration test (insert via AsyncEntryStore, verify via Store::get).

### IR-03: rmcp Tool Macro + schemars JSON Schema Generation

The `#[tool_router]` macro auto-generates JSON Schema from Rust types via schemars. If schemars doesn't generate the expected schema (e.g., `Option<String>` doesn't become a non-required param), tool discovery breaks.

**Scenarios**: Call `tools/list`, inspect the JSON Schema for each tool. Verify required vs optional params. Verify type annotations.

### IR-04: Signal Handling + rmcp Session Lifecycle

SIGTERM/SIGINT must interact correctly with rmcp's `waiting()` future. If signal handling cancels `waiting()` but rmcp doesn't clean up, the shutdown sequence may hang.

**Scenarios**: Start server, send SIGTERM. Verify server exits within 10 seconds. Verify no zombie process.

### IR-05: VectorIndex::load() with Existing Store

The server opens Store first, then loads VectorIndex with `Arc<Store>`. VectorIndex::load() reads VECTOR_MAP from Store. If Store changes (new tables added) affect transaction behavior, load may fail.

**Scenarios**: Create Store with new tables. Insert entries + vectors. Dump. Reopen Store. Load VectorIndex. Verify loaded correctly.

## Edge Cases

### EC-01: Empty Project (No Entries, No Vectors)

First-run scenario: database is empty, vector index is empty, embedding model may not be downloaded. Server must start successfully and respond to all MCP requests.

### EC-02: Project Root is Filesystem Root

If cwd is `/` and there's no `.git/`, project root becomes `/`. Hash is deterministic but unusual. Data directory is valid.

### EC-03: Very Long Project Path

Paths over 1000 characters are valid on some filesystems. SHA-256 handles arbitrary input length. The hash is fixed at 16 chars regardless.

### EC-04: Unicode in Project Path

Project root paths containing Unicode characters (e.g., `/home/\u{30e6}\u{30cb}/project`) must hash correctly. SHA-256 operates on UTF-8 bytes.

### EC-05: agent_id with Special Characters

agent_id strings containing spaces, Unicode, or special characters must be stored and retrieved correctly from AGENT_REGISTRY. redb `&str` keys handle arbitrary valid UTF-8.

### EC-06: Multiple Rapid Server Starts

If Claude Code restarts the server rapidly (crash + restart), the database must handle re-open cleanly. redb's lock file prevents concurrent opens.

### EC-07: Database File Locked by Previous Instance

If a previous server instance didn't exit cleanly (SIGKILL), the redb lock file may still exist. `Store::open()` should either wait or fail with a clear error.

## Security Risks

### SR-01: Agent Identity Spoofing via agent_id Parameter

**Untrusted input:** The `agent_id` tool parameter is self-reported and unverified on stdio.
**Damage potential:** A malicious agent claims to be "human" (Privileged trust), bypassing capability restrictions.
**Blast radius:** When vnc-002 enforces capabilities, spoofing would grant unauthorized write access.
**Mitigation (vnc-001):** Audit log records all claimed identities for forensic analysis. The threat model acknowledges self-reported identity on stdio (ADR-003).
**Mitigation (future):** HTTPS + OAuth 2.1 provides verified identity. Internal pipeline unchanged.

### SR-02: Path Traversal in Project Directory

**Untrusted input:** `--project-dir` CLI argument.
**Damage potential:** Attacker passes `--project-dir /etc/` causing Unimatrix to create files in sensitive directories.
**Blast radius:** Limited -- creates `~/.unimatrix/{hash}/` based on the hash, not the raw path. The data directory is always under `~/.unimatrix/`.
**Mitigation:** Canonicalize the path before use. Data directory is always `~/.unimatrix/{hash}/`, never a user-controlled path.

### SR-03: Audit Log Denial via Rapid Requests

**Untrusted input:** Rapid tool calls from agents.
**Damage potential:** Audit log grows unbounded. Disk full. Server becomes unresponsive.
**Blast radius:** Single project data directory.
**Mitigation (vnc-001):** None -- this is mitigated by rate limiting in future features (crt-001).
**Mitigation (design):** Audit log uses monotonic u64 IDs. redb transactions are crash-safe. Even rapid writes don't corrupt data.

### SR-04: Deserialization of Malformed AgentRecord

**Untrusted input:** Data read from AGENT_REGISTRY table (could be corrupted on disk).
**Damage potential:** bincode deserialization failure crashes the server.
**Blast radius:** Server process crash, MCP session lost.
**Mitigation:** Deserialize in `Result`-returning methods. Map deserialization errors to ServerError, not panics.

## Failure Modes

### FM-01: Embedding Model Unavailable

**Trigger:** HuggingFace Hub unreachable or model download fails.
**Expected behavior:** EmbedServiceHandle transitions to Failed. context_lookup and context_get work normally. context_search returns error -32004 with message suggesting context_lookup as alternative.
**Recovery:** Restart server when network is available. Model caches at `~/.cache/unimatrix/models/` so second attempt may succeed from cache.

### FM-02: Database Corrupted

**Trigger:** Disk failure, SIGKILL during write.
**Expected behavior:** Store::open() returns error. Server logs error and exits with non-zero code.
**Recovery:** Manual intervention required. redb's crash safety should prevent most corruption.

### FM-03: Vector Index Corrupted

**Trigger:** SIGKILL between dump file writes (graph written, data not).
**Expected behavior:** VectorIndex::load() returns error. Server creates fresh empty index. Existing entries remain in store but vector search returns no results.
**Recovery:** Future: rebuild-index command re-embeds all entries. vnc-001: manual workaround by deleting vector/ directory.

### FM-04: Audit Log Write Failure

**Trigger:** Disk full, redb internal error.
**Expected behavior:** Log warning to stderr. Continue processing the tool call. The tool response is not affected by audit failure.
**Recovery:** Clear disk space. Audit events during the outage are lost but tool operations continue.

### FM-05: Arc::try_unwrap Fails During Shutdown

**Trigger:** Bug in reference management -- an Arc<Store> clone is leaked.
**Expected behavior:** Log warning "skipping compact: outstanding Store references". Exit with code 0. Database is valid (redb crash-safe) but may be larger than necessary.
**Recovery:** Compact runs on next clean shutdown. No data loss.

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 5 (R-01, R-04, R-06, R-11, R-12, R-14) | 26 scenarios |
| High | 7 (R-02, R-03, R-05, R-08, R-09, R-10, R-13, R-15, R-16) | 30 scenarios |
| Medium | 2 (R-07) | 4 scenarios |
| Low | 0 | 0 scenarios |
| **Total** | **16 risks** | **60 scenarios** |
