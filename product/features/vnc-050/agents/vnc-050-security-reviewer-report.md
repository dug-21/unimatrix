# Security Review: vnc-050-security-reviewer

## Risk Level: low

## Summary
Scoped bearer tokens (tokens.json), slug guard, token-mode capability ceiling, observe SessionWrite gate, CA-trust client path. Auth core is sound: constant-time full scan, fail-closed on corrupt/missing table, no Admin grant, single slug guard ahead of resolve_store, no default-slug fallback. No new crates/npm deps, no secrets. Two non-blocking findings.

## Findings

### F1 — Env-sourced observe URL may be plain http (token in cleartext)
- **Severity**: medium
- **Location**: packages/unimatrix/lib/hook-client/remote-resolve.js:85-98; transport-http.js:75-79
- **Description**: `UNIMATRIX_URL` / `UNIMATRIX_MCP_URL` are not scheme-validated. The MCP bridge forces https, but the observe transport accepts `http:` and sends `Authorization: Bearer` unencrypted. Headless/CI operators misconfiguring the URL leak a scoped token.
- **Recommendation**: Reject non-`https:` env URLs in resolveRemote (bundle path already enforces https).
- **Blocking**: no

### F2 — tokens.json stat on every authenticated request
- **Severity**: low
- **Location**: crates/unimatrix-server/src/http/token_table.rs `scan` -> `refresh`
- **Description**: One metadata syscall per request; cheap, fail-closed on error. Not exploitable beyond minor load. File permissions of an operator-edited tokens.json are not checked (writer uses 0600).
- **Recommendation**: Optional: warn at boot if tokens.json is group/world-readable.
- **Blocking**: no

## Verified
- Validator: hex/len early reject only; legacy ct_eq + full CT scan always run; tombstone == unknown (401 identical). LegacyDisabled has a distinct 401 body (intentional per FR-11; reveals the legacy token was valid — acceptable, holder already has it).
- Slug guard runs before observe/MCP split and before resolve_store; missing principal on slug path -> 500 (fail closed); 403 body fixed; denial audited to home slug; audit rows carry label only.
- parse_project_key has no default alias; non-slug paths 404 downstream.
- Cap ceiling: Parts without principal -> empty ceiling (deny all); every require_cap call site migrated; Admin never in any mode; learning signals suppressed for read tokens.
- `token:` agent_id prefix reserved (case/whitespace-insensitive) on stdio and HTTP.
- Writer: flock, create_new 0600 tmp, fsync, atomic rename, OsRng, labels never reusable.
- Legacy env override: strict values, boot error on bad value, fallback config cannot re-enable legacy.
- Client: CA branch forces rejectUnauthorized:true, forbids checkServerIdentity override, defers token-bearing body until verified; reserved headers cannot be overridden by UNIMATRIX_HEADERS.

## Blast Radius Assessment
Worst case: a bug in slug guard/ceiling lets a scoped token read or write another project's store (cross-tenant disclosure/corruption). Mitigated by guard-before-funnel ordering plus spies in tests. A table bug fails closed (scoped tokens 401), legacy unaffected.

## Regression Risk
Low. Legacy token path field-identical (FR-39 test). Config load fallback now errors if the env override is invalid (intended). client-bundle CLI args reshaped; old `client-bundle <slug>` form covered by tests.

## PR Comments
- Posted 1 comment on PR #1022
- Blocking findings: no

## Knowledge Stewardship
- Stored: nothing novel to store -- findings PR-specific
