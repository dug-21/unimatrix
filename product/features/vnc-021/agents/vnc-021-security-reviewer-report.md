# Security Review: vnc-021-security-reviewer

## Risk Level: low

## Summary
No blocking security findings. Two medium-severity items identified for follow-up: (1) token file TOCTOU race in permission setting, (2) body size limit bypassed via chunked transfer encoding. All critical security controls verified correct: constant-time token comparison, exact-match health bypass, pre-TLS connection limiting with RAII guards, no secrets in code/logs.

## Findings

### Finding 1: Token File TOCTOU Race Window
- **Severity**: medium
- **Location**: crates/unimatrix-server/src/http/token.rs:44-59
- **Description**: File created with default umask permissions, then set to 0600. Brief window where file exists with overly permissive mode.
- **Recommendation**: Use `OpenOptionsExt::mode(0o600)` with `create_new(true)` for atomic creation.
- **Blocking**: no

### Finding 2: Body Size Limit Header-Only Check
- **Severity**: medium
- **Location**: crates/unimatrix-server/src/http/router.rs:292-297
- **Description**: Body size enforcement checks Content-Length header only. Chunked transfer encoding bypasses the check.
- **Recommendation**: Wrap body in `http_body::Limited` adapter before passing to rmcp.
- **Blocking**: no

### Finding 3: Connection Timeout Covers Full Lifetime
- **Severity**: low
- **Location**: crates/unimatrix-server/src/http/listener.rs:135-147
- **Description**: 30s timeout wraps entire connection, not per-idle-period. Long SSE/MCP sessions may be killed.
- **Recommendation**: Document; consider per-idle timeout in future iteration.
- **Blocking**: no

### Finding 4: CallerId::HttpBearer Dead Code
- **Severity**: low
- **Location**: crates/unimatrix-server/src/services/mod.rs:79
- **Description**: Variant defined but never constructed. HTTP requests use CallerId::Agent via existing path.
- **Recommendation**: Acceptable structural preparation. Remove if unused after W2-6.
- **Blocking**: no

## Blast Radius Assessment
Worst case: auth bypass via path-match flaw grants unauthenticated MCP access. Mitigated by exact-match semantics and test coverage. All failure modes are safe (error returns, not silent corruption). HTTP transport is purely additive; existing UDS/stdio paths unmodified.

## Regression Risk
Low. No existing behavior modified. HTTP disabled by default. Config/shutdown/main changes are additive fields with backward-compatible defaults. Compiler enforces exhaustive match on new CallerId variant.

## PR Comments
- Posted detailed review comment on PR #661
- Posted approval decision comment on PR #661
- Blocking findings: no

## Knowledge Stewardship
- Nothing novel to store -- the TOCTOU pattern already exists as Unimatrix lesson #665. The Content-Length-only body check is a one-off finding specific to this PR.
