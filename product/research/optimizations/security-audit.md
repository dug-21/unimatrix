# Unimatrix OWASP Top 10 Security Audit Report

**Date:** 2026-03-03
**Auditor:** Claude Opus 4.6 (automated static analysis)
**Scope:** All Rust source code in `crates/unimatrix-{store,vector,embed,core,server,engine,adapt,observe}/`
**Methodology:** Manual source code review against OWASP Top 10 (adapted for backend/API applications)

## Executive Summary

The Unimatrix codebase demonstrates strong security fundamentals: all crates use `#![forbid(unsafe_code)]`, input validation is comprehensive with dedicated length and control-character checks, content scanning detects prompt injection and PII patterns, and a layered authentication model protects the UDS IPC channel. The most significant findings relate to the self-asserted agent identity model in the MCP layer (agent_id is a client-provided string with no cryptographic verification), missing rate limiting on write operations, and the absence of encryption at rest for the redb database. No critical vulnerabilities were identified; findings are predominantly medium-severity defense-in-depth gaps appropriate for a local-only knowledge engine.

---

## Findings Summary

| ID | OWASP Category | Severity | File:Line | Description |
|----|---------------|----------|-----------|-------------|
| F-01 | A01: Broken Access Control | Medium | `server/src/identity.rs:22-34` | Agent identity is self-asserted via `agent_id` parameter; no cryptographic verification |
| F-02 | A01: Broken Access Control | Medium | `server/src/registry.rs:200-210` | Auto-enrollment grants Read+Search to any unknown agent_id without approval |
| F-03 | A01: Broken Access Control | Low | `server/src/registry.rs:793-803` | Protected agent check is case-sensitive; "SYSTEM" bypasses protection for "system" |
| F-04 | A01: Broken Access Control | Low | `server/src/tools.rs` (context_status) | `maintain=true` triggers write operations (confidence refresh, compaction) but only requires Read capability |
| F-05 | A02: Cryptographic Failures | Medium | `store/src/db.rs` | Database at rest is unencrypted; redb files are readable by anyone with filesystem access |
| F-06 | A02: Cryptographic Failures | Info | `store/src/hash.rs:7-16` | SHA-256 used for content dedup (appropriate); no HMAC or signing for integrity verification |
| F-07 | A03: Injection | Low | `server/src/scanning.rs` | Content scanning is regex-based; sophisticated evasion (Unicode homoglyphs, zero-width chars) may bypass detection |
| F-08 | A03: Injection | Low | `server/src/uds_listener.rs:47-59` | Session ID sanitization allows `-` and `_` but no path traversal check; session_id used only as HashMap key (safe) |
| F-09 | A04: Insecure Design | Medium | `server/src/tools.rs` | No rate limiting on write operations (context_store, context_correct); a compromised agent could flood the knowledge base |
| F-10 | A04: Insecure Design | Medium | `server/src/identity.rs:22-34` | Default identity "anonymous" gets auto-enrolled with Read+Search; all unauthenticated requests succeed for reads |
| F-11 | A04: Insecure Design | Low | `server/src/audit.rs:109-141` | Audit log scan (`write_count_since`) does full table iteration; no index on agent_id or timestamp |
| F-12 | A04: Insecure Design | Info | `engine/src/wire.rs:16` | 1 MiB maximum payload size is generous; could be used for memory pressure attacks on the UDS listener |
| F-13 | A05: Security Misconfiguration | Low | `server/src/main.rs:88` | Default log level is "info"; no structured security event logging separate from operational logs |
| F-14 | A05: Security Misconfiguration | Info | `server/src/main.rs:241-242` | MCP served over stdio; relies on parent process (Claude Code) for transport security |
| F-15 | A06: Vulnerable Components | Low | `embed/Cargo.toml:12-13` | `ort` and `ort-sys` pinned to release candidate (`=2.0.0-rc.9`); RC versions may have unpatched issues |
| F-16 | A06: Vulnerable Components | Info | `server/Cargo.toml:28` | `rmcp` pinned to exact version `=0.16.0`; prevents automatic minor/patch security updates |
| F-17 | A07: Auth Failures | Medium | `engine/src/auth.rs:113-119` | Process lineage check (Layer 3) is advisory only; failure is logged but connection is allowed |
| F-18 | A07: Auth Failures | Low | `engine/src/auth.rs:138-144` | Process lineage check accepts any non-empty cmdline; provides no meaningful verification |
| F-19 | A08: Data Integrity | Low | `server/src/registry.rs:430-442` | bincode deserialization of AgentRecord from untrusted storage; malformed bytes could cause unexpected errors |
| F-20 | A08: Data Integrity | Info | All Cargo.toml | No `Cargo.lock` audit via `cargo-audit` in CI pipeline (not verified but noted) |
| F-21 | A09: Logging Gaps | Medium | `server/src/tools.rs:411-420` | Audit logging is best-effort (`let _ = self.audit.log_event(...)`); failures silently discarded |
| F-22 | A09: Logging Gaps | Low | `server/src/audit.rs` | No audit log rotation or size cap; unbounded growth on long-running instances |
| F-23 | A09: Logging Gaps | Low | `server/src/uds_listener.rs` | UDS authentication failures logged via `tracing::warn` but not written to the audit log table |
| F-24 | A10: SSRF | Info | `embed/src/download.rs:29-42` | Model download from HuggingFace Hub; URL is hardcoded via `hf-hub` crate (not user-controlled) |

---

## Detailed Findings by OWASP Category

### A01: Broken Access Control

**F-01: Self-Asserted Agent Identity (Medium)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/identity.rs:22-34`

The `agent_id` parameter on every MCP tool call is a client-provided optional string. There is no cryptographic verification, token, or session binding. Any caller can claim to be any agent:

```rust
pub fn extract_agent_id(agent_id: &Option<String>) -> String {
    match agent_id {
        Some(id) => {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                "anonymous".to_string()
            } else {
                trimmed.to_string()
            }
        }
        None => "anonymous".to_string(),
    }
}
```

A malicious or compromised MCP client can pass `agent_id: "human"` to gain Privileged trust with full Admin capabilities. The UDS channel does have UID-based authentication (Layer 2), but the MCP stdio channel relies entirely on the calling process being trusted.

**Impact:** An attacker with access to the stdio MCP channel can impersonate any enrolled agent, including "human" (Privileged, all capabilities) or any agent with Admin rights.

**Mitigating factor:** The MCP server runs over stdio, so only the parent process (Claude Code) can communicate with it. The UDS channel authenticates via peer credentials.

---

**F-02: Auto-Enrollment of Unknown Agents (Medium)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/registry.rs:200-210`

Any unknown `agent_id` is automatically enrolled with `Restricted` trust and `[Read, Search]` capabilities:

```rust
let new_agent = AgentRecord {
    agent_id: agent_id.to_string(),
    trust_level: TrustLevel::Restricted,
    capabilities: vec![Capability::Read, Capability::Search],
    ...
};
```

This is by design for usability, but means any string passed as `agent_id` gains read access to the entire knowledge base without any registration or approval process.

---

**F-03: Case-Sensitive Protected Agent Check (Low)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/registry.rs:793-803`

The protected agent check in `enroll_agent` is case-sensitive:

```rust
const PROTECTED_AGENTS: &[&str] = &["system", "human"];

if PROTECTED_AGENTS.contains(&target_id) {
    return Err(ServerError::ProtectedAgent { ... });
}
```

A test confirms `"SYSTEM"` (uppercase) is allowed as a target. While this is documented behavior (case-sensitive IDs), it means an attacker could create a "SYSTEM" or "Human" agent with elevated privileges, potentially causing confusion in audit logs or administrative interfaces.

---

**F-04: Maintenance Operations Without Write Capability (Low)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/tools.rs` (context_status handler)

The `context_status` tool with `maintain=true` triggers write operations (confidence refresh batch of 100, graph compaction, co-access cleanup) but the capability check only requires `Read`:

```rust
// context_status capability check (from tools.rs)
self.registry
    .require_capability(&identity.agent_id, Capability::Read)
    .map_err(rmcp::ErrorData::from)?;
```

Any agent with Read capability can trigger maintenance writes to the database. This should require at minimum Write or Admin capability when `maintain=true`.

---

### A02: Cryptographic Failures

**F-05: No Encryption at Rest (Medium)**

File: `/workspaces/unimatrix/crates/unimatrix-store/src/db.rs`

The redb database file is stored unencrypted at `~/.unimatrix/{hash}/unimatrix.redb`. The data directory is created with `0o700` permissions (good), but:

- The database file itself has no encryption
- Any process running as the same UID can read the database
- A local attacker with file read access can extract all knowledge entries, agent records, audit logs, and embeddings

The directory permission (`0o700` set in `project.rs:86`) provides filesystem-level protection:

```rust
#[cfg(unix)]
fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o700))?;
```

This is appropriate for a local-only tool, but the database contents include potentially sensitive organizational knowledge.

---

**F-06: Content Hash Without HMAC (Info)**

File: `/workspaces/unimatrix/crates/unimatrix-store/src/hash.rs:7-16`

SHA-256 is used for content deduplication hashing. This is appropriate for the dedup use case. There is no HMAC or digital signature for data integrity verification, meaning a direct database editor could modify entries without detection. This is an observation, not a vulnerability, given the threat model.

---

### A03: Injection

**F-07: Regex-Based Content Scanning Limitations (Low)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/scanning.rs`

The `ContentScanner` implements 25+ injection patterns and 6 PII patterns. This is a strong defense, but regex-based approaches have known limitations:

1. **Unicode homoglyphs:** Characters like Cyrillic "а" (U+0430) replacing Latin "a" could bypass pattern matching for words like "ignore" or "system"
2. **Zero-width characters:** U+200B (zero-width space) inserted between pattern words would evade detection
3. **Token-level attacks:** Patterns split across content and title fields may not be detected (title scanning only checks injection patterns, not PII)
4. **Encoded payloads:** While URL encoding and HTML entities are detected, base64-encoded payloads without the "base64 decode:" prefix would pass

The validation module does reject control characters (U+0000-U+001F) except newline and tab in content fields, which mitigates some encoding attacks.

---

**F-08: Session ID Used Safely (Low)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/uds_listener.rs:47-59`

Session IDs are sanitized to `[a-zA-Z0-9-_]` with a 128-character limit:

```rust
fn sanitize_session_id(session_id: &str) -> Result<(), String> {
    if session_id.is_empty() { return Err(...); }
    if session_id.len() > 128 { return Err(...); }
    for ch in session_id.chars() {
        if !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' {
            return Err(...);
        }
    }
    Ok(())
}
```

Session IDs are used as HashMap keys (safe) and stored in redb tables. No path construction or command execution uses session IDs. This is well-implemented.

---

### A04: Insecure Design

**F-09: No Rate Limiting on Write Operations (Medium)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/tools.rs`

There is no rate limiting on `context_store` or `context_correct` operations. The audit module has `write_count_since` which could be used for rate limiting, but it is not called in the write path:

```rust
// audit.rs provides this capability:
pub fn write_count_since(&self, agent_id: &str, since: u64) -> Result<u64, ServerError>

// But context_store does NOT call it before proceeding with the write
```

A compromised agent with Write capability could:
- Flood the knowledge base with entries, degrading search quality
- Exhaust disk space via the redb database
- Overwhelm the embedding pipeline with embedding requests

The signal queue has a 10,000-record cap (`db.rs:122-135`), but the main ENTRIES table and AUDIT_LOG have no such bounds.

---

**F-10: Anonymous Default Access (Medium)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/identity.rs:22-34`

When no `agent_id` is provided (or an empty string is given), the identity resolves to "anonymous" which auto-enrolls with Read+Search capabilities. This means:

- All read operations succeed without any identification
- Search queries return full knowledge base results
- The system cannot distinguish between different unauthenticated callers in audit logs

This is by design for the MCP stdio use case but represents a defense-in-depth gap.

---

**F-11: Audit Log Full-Table Scan (Low)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/audit.rs:109-141`

`write_count_since` iterates the entire AUDIT_LOG table:

```rust
for result in table.iter() { ... }
```

On a long-running instance with many audit events, this becomes a performance concern that could be exploited for denial-of-service if the function were called in a request path. Currently it is not called in production request handling, but it exists as a public API.

---

**F-12: Generous Payload Size Limit (Info)**

File: `/workspaces/unimatrix/crates/unimatrix-engine/src/wire.rs:16`

The maximum wire payload is 1 MiB (`MAX_PAYLOAD_SIZE = 1_048_576`). While bounded, this is generous for JSON hook messages and could be used for memory pressure on the UDS listener. The content validation layer also caps content at 50,000 characters (`MAX_CONTENT_LEN`), which provides a tighter bound for tool parameters.

---

### A05: Security Misconfiguration

**F-13: No Separate Security Event Logging (Low)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/main.rs:88`

Logging uses `tracing_subscriber` with a simple env filter ("info" or "debug"). Security events (authentication failures, capability denials, content scan rejections) are mixed with operational logs. There is no separate security event stream or alerting mechanism.

---

**F-14: Stdio Transport Security Delegation (Info)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/main.rs:241-242`

The MCP server communicates over stdio:

```rust
let running = server
    .serve(rmcp::transport::io::stdio())
    .await
```

This relies entirely on the parent process (Claude Code) for transport security. This is the expected MCP server pattern but means the server has no independent transport-layer authentication for MCP clients.

---

### A06: Vulnerable and Outdated Components

**F-15: ONNX Runtime Release Candidate (Low)**

File: `/workspaces/unimatrix/crates/unimatrix-embed/Cargo.toml:12-13`

```toml
ort = { version = "=2.0.0-rc.9", default-features = false }
ort-sys = "=2.0.0-rc.9"
```

The `ort` (ONNX Runtime) crate is pinned to a release candidate. RC versions may contain bugs or security issues that are resolved in stable releases. The `ort-sys` crate wraps native C++ code (ONNX Runtime), which is the most likely vector for memory safety issues in the dependency tree (outside the Rust `#![forbid(unsafe_code)]` boundary).

---

**F-16: Exact Version Pinning Prevents Patch Updates (Info)**

File: `/workspaces/unimatrix/crates/unimatrix-server/Cargo.toml:28`

```toml
rmcp = { version = "=0.16.0", features = ["server", "transport-io", "macros"] }
```

Exact version pinning (`=0.16.0`) prevents automatic adoption of patch-level security fixes. This is a deliberate trade-off for stability but means security patches in rmcp require manual version bumps.

**Dependency overview (all workspace Cargo.toml files):**
- `redb = "3.1"` -- embedded database, well-maintained
- `serde/serde_json` -- standard, widely audited
- `bincode = "2"` -- used for all internal serialization; version 2 is current
- `tokio = "1"` -- standard async runtime
- `sha2 = "0.10"` -- RustCrypto, widely audited
- `nix = "0.31"` -- Unix API wrapper, maintained
- `hf-hub = "0.4"` -- HuggingFace Hub download client (network access)
- `tokenizers = "0.21"` -- HuggingFace tokenizer (C++ FFI)
- `regex = "1"` -- well-audited
- `clap = "4"`, `dirs = "6"`, `fs2 = "0.4"`, `tracing = "0.1"` -- standard utilities

No known critical CVEs were identified in the pinned dependency versions at time of review, but a `cargo audit` check should be run regularly.

---

### A07: Identification and Authentication Failures

**F-17: Advisory Process Lineage Check (Medium)**

File: `/workspaces/unimatrix/crates/unimatrix-engine/src/auth.rs:113-119`

The 3-layer authentication model for UDS connections has a fundamental weakness in Layer 3:

```rust
// Layer 3: Process lineage (advisory, Linux only)
#[cfg(target_os = "linux")]
if let Some(pid) = creds.pid {
    if let Err(e) = verify_process_lineage(pid) {
        tracing::warn!("process lineage check failed for pid {pid}: {e} (advisory, allowing connection)");
    }
}
```

Layer 3 is explicitly advisory -- failures are logged but do not reject connections. This means any process running as the same UID can connect to the UDS socket, even if it is not a Claude Code hook process.

Layer 2 (UID verification) is the actual security boundary:

```rust
if creds.uid != server_uid {
    return Err(AuthError::UidMismatch { expected: server_uid, actual: creds.uid });
}
```

This is appropriate for a single-user development tool but would be insufficient in multi-tenant environments.

---

**F-18: Permissive Process Lineage Verification (Low)**

File: `/workspaces/unimatrix/crates/unimatrix-engine/src/auth.rs:138-144`

```rust
fn verify_process_lineage(pid: u32) -> Result<(), AuthError> {
    let cmdline = fs::read_to_string(&cmdline_path)?;
    if cmdline.is_empty() {
        return Err(AuthError::LineageFailed("empty cmdline".to_string()));
    }
    Ok(())
}
```

The lineage check only verifies that `/proc/{pid}/cmdline` is non-empty. Any process with a non-empty cmdline passes. The comment notes "Future versions may verify the process descends from a Claude Code session" but this is not implemented.

---

### A08: Software and Data Integrity Failures

**F-19: Bincode Deserialization from Storage (Low)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/registry.rs:430-442`

AgentRecord deserialization from the redb database uses bincode:

```rust
fn deserialize_agent(bytes: &[u8]) -> Result<AgentRecord, ServerError> {
    let (record, _) = bincode::serde::decode_from_slice::<AgentRecord, _>(
        bytes, bincode::config::standard(),
    ).map_err(|e| ServerError::Registry(format!("deserialization failed: {e}")))?;
    Ok(record)
}
```

Bincode v2 with `serde` path is used throughout (AgentRecord, AuditEvent, EntryRecord, InjectionLogRecord, SignalRecord). While bincode v2 is generally safe against deserialization attacks, malformed data in the redb file (due to corruption or direct modification) could produce unexpected AgentRecord values. The error is properly handled (returns `Err`), but there is no secondary integrity check (e.g., HMAC) on serialized records.

Mitigating factors:
- All crates use `#![forbid(unsafe_code)]`
- The database file has `0o700` directory permissions
- bincode v2 does not support arbitrary code execution through deserialization

---

**F-20: No Dependency Audit in CI (Info)**

No evidence of `cargo audit` being run in CI was found. The workspace `Cargo.toml` and individual crate manifests pin dependencies but do not include audit tooling configuration.

---

### A09: Security Logging and Monitoring Failures

**F-21: Best-Effort Audit Logging (Medium)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/tools.rs:411-420`

Audit events are logged with `let _ =`, silently discarding failures:

```rust
let _ = self.audit.log_event(AuditEvent {
    event_id: 0,
    timestamp: 0,
    session_id: String::new(),
    agent_id: identity.agent_id.clone(),
    operation: "context_search".to_string(),
    target_ids: target_ids.clone(),
    outcome: Outcome::Success,
    detail: format!("returned {} results", results_with_scores.len()),
});
```

This pattern is used for all search/lookup audit events. Write operations (context_store, context_correct) use atomic audit within the same transaction (`insert_with_audit`), which is better, but read-path auditing can silently fail.

Additionally, `session_id` is always `String::new()` in MCP tool audit events, making it impossible to correlate MCP tool calls to specific sessions.

---

**F-22: Unbounded Audit Log Growth (Low)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/audit.rs`

The AUDIT_LOG table has no size cap, rotation, or archival mechanism. Unlike the SIGNAL_QUEUE (capped at 10,000 records), audit events accumulate indefinitely. On a heavily-used instance, this could consume significant disk space and degrade the performance of `write_count_since` scans.

---

**F-23: UDS Authentication Failures Not in Audit Log (Low)**

File: `/workspaces/unimatrix/crates/unimatrix-server/src/uds_listener.rs`

When a UDS connection fails UID verification, the failure is logged via `tracing::warn` but not recorded in the persistent AUDIT_LOG table. This means authentication failures are only visible in stderr logs, which may be lost on process restart.

---

### A10: Server-Side Request Forgery (SSRF)

**F-24: Model Download from HuggingFace Hub (Info)**

File: `/workspaces/unimatrix/crates/unimatrix-embed/src/download.rs:29-42`

```rust
let api = hf_hub::api::sync::Api::new()?;
let repo = api.model(model.model_id().to_string());
let downloaded_onnx = repo.get(model.onnx_repo_path())?;
```

The embedding model is downloaded from HuggingFace Hub on first use. The model ID is hardcoded in the `EmbeddingModel` enum (not user-controlled), so this is not a traditional SSRF vector. However:

- The download occurs over HTTPS to HuggingFace servers
- The `hf-hub` crate may follow redirects
- No content integrity verification (SHA-256 hash check) of the downloaded ONNX model is performed beyond checking file size > 0

This is acceptable for the current use case but noted for completeness.

---

## Positive Security Observations

The following security measures are well-implemented and deserve recognition:

1. **`#![forbid(unsafe_code)]`** is declared in all 8 crate lib.rs files, providing strong memory safety guarantees within Rust code boundaries.

2. **Comprehensive input validation** (`validation.rs`): All tool parameters have length limits, control character rejection, and type validation. The validation module has 50+ unit tests.

3. **Content scanning** (`scanning.rs`): 25+ injection patterns and 6 PII patterns with singleton compilation. Both content body and titles are scanned.

4. **UDS authentication** (`auth.rs`): Three-layer model with filesystem permissions (Layer 1), UID verification (Layer 2), and advisory lineage check (Layer 3). Layer 2 is enforced.

5. **Directory permissions** (`project.rs:86`): Data directory created with `0o700`, restricting access to the owner.

6. **PID guard with flock** (`pidfile.rs`): RAII-based exclusive advisory lock prevents multiple server instances, with identity verification before SIGTERM.

7. **Protected bootstrap agents** (`registry.rs:70`): "system" and "human" cannot be modified via enrollment API.

8. **Self-lockout prevention** (`registry.rs:341`): Admin cannot remove their own Admin capability.

9. **Capability-based access control**: Every tool handler follows the pattern: identity -> capability check -> validation -> business logic -> audit.

10. **Poison recovery** (`categories.rs`): `unwrap_or_else(|e| e.into_inner())` on all RwLock/Mutex operations prevents panics from poisoned locks.

11. **Wire protocol bounds** (`wire.rs`): Length-prefixed framing with 1 MiB maximum prevents unbounded reads. Zero-length payloads are rejected.

12. **Session ID sanitization** (`uds_listener.rs:47-59`): Strict allowlist for session ID characters.

13. **Signal queue cap** (`db.rs:122-135`): 10,000-record cap with oldest-first eviction prevents unbounded growth.

---

## Prioritized Remediation Recommendations

### Priority 1 (Address in next milestone)

| ID | Recommendation | Effort |
|----|---------------|--------|
| R-01 | **Add rate limiting for write operations.** Implement a per-agent, time-windowed write limit using the existing `write_count_since` audit capability. Reject writes exceeding N operations per hour. | Medium |
| R-02 | **Require Admin capability for `maintain=true` on context_status.** The maintenance path triggers writes (confidence refresh, graph compaction); Read-only agents should not trigger these. | Low |
| R-03 | **Persist security events to audit log.** UDS authentication failures, capability denials, and content scan rejections should be written to AUDIT_LOG, not just tracing. | Medium |

### Priority 2 (Address in upcoming release)

| ID | Recommendation | Effort |
|----|---------------|--------|
| R-04 | **Add audit log rotation/compaction.** Implement a configurable size cap or time-based archival for the AUDIT_LOG table. | Medium |
| R-05 | **Populate session_id in MCP audit events.** Use the rmcp session context (if available) or a server-generated session ID to correlate MCP tool calls. | Low |
| R-06 | **Upgrade `ort` to stable release.** When `ort` 2.0 stable is released, upgrade from the RC to get any security fixes in the C++ ONNX Runtime layer. | Low |
| R-07 | **Add `cargo audit` to CI.** Run `cargo audit` in the build pipeline to catch known vulnerabilities in dependencies. | Low |

### Priority 3 (Consider for future hardening)

| ID | Recommendation | Effort |
|----|---------------|--------|
| R-08 | **Case-insensitive protected agent check.** Normalize agent IDs to lowercase before comparing against PROTECTED_AGENTS to prevent confusion attacks. | Low |
| R-09 | **Enhanced content scanning.** Add Unicode normalization (NFC/NFKD) before regex matching to detect homoglyph-based evasion. Consider zero-width character stripping. | Medium |
| R-10 | **Model integrity verification.** Verify SHA-256 hash of downloaded ONNX model files against a pinned manifest to prevent supply-chain attacks on the embedding model. | Medium |
| R-11 | **Audit log tamper detection.** Add an HMAC chain to audit events so that deletions or modifications of historical events can be detected. | High |
| R-12 | **Database encryption at rest.** Evaluate redb encryption options or filesystem-level encryption (LUKS, FileVault) for the data directory. | High |

---

## Methodology Notes

- **Static analysis only.** No dynamic testing, fuzzing, or penetration testing was performed.
- **Scope:** All `.rs` files in `crates/unimatrix-{store,vector,embed,core,server,engine,adapt,observe}/` and all `Cargo.toml` files.
- **Threat model assumed:** Single-user local development tool accessed by Claude Code and its hook processes. The primary trust boundary is the Unix domain socket (same-UID processes).
- **Files read:** 60+ Rust source files and 10 Cargo.toml files were reviewed in their entirety.
