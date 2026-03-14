# Security Review: bugfix-252-security-reviewer

## Risk Level: low

## Summary

PR #254 makes two changes to `context_status`: it lowers the capability gate from `Admin` to `Read`, and it removes the dead `maintain` parameter and field from `StatusParams`. Both changes are safe. The gate change is appropriate because `context_status` is a read-only diagnostic tool and `Read` capability is the minimum auto-enrolled capability in production. The `maintain` removal eliminates dead code that was silently ignored since col-013; it does not change runtime behaviour. No mutation paths, no new external inputs, no new dependencies.

## Findings

### Finding 1: Gate widening from Admin to Read (access control)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:774`
- **Description**: `require_cap(&ctx.agent_id, Capability::Admin)` is replaced with `require_cap(&ctx.agent_id, Capability::Read)`. `context_status` has no write path — it only reads counts, metrics, distributions, tick metadata, and the embedding consistency scan. The gate change is consistent with peer read-only tools (`context_lookup`, `context_get`, `context_briefing`) which all require `Read`. Auto-enrolled agents already receive `Read` in both `PERMISSIVE_AUTO_ENROLL=true` (Read+Write+Search) and `PERMISSIVE_AUTO_ENROLL=false` (Read+Search) paths, so the effective enforcement boundary is unchanged in either mode.
- **Recommendation**: No action required. The gate level is correct for a read-only tool.
- **Blocking**: no

### Finding 2: PERMISSIVE_AUTO_ENROLL=true in production code (pre-existing, informational)

- **Severity**: low (pre-existing, not introduced by this PR)
- **Location**: `crates/unimatrix-server/src/infra/registry.rs:27`
- **Description**: `const PERMISSIVE_AUTO_ENROLL: bool = true;` grants Write to every unknown agent. This is not introduced by this PR, but the gate change to Read draws attention to the fact that any unenrolled agent already had Write and Search. Admin remains the only capability that is never auto-granted. The gate change does not worsen this posture.
- **Recommendation**: Consider making this a runtime config flag rather than a compile-time constant before production hardening. Tracked separately from this PR.
- **Blocking**: no

### Finding 3: StatusReport data disclosure surface (informational)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/status.rs`
- **Description**: The status report includes entry counts by category/topic, correction chain counts, co-access cluster data, confidence score distributions, stale pair counts, and tick metadata. This is internal health data, not user-supplied content. No entry IDs, entry text, or embedding vectors are returned by `compute_report`. The `check_embeddings` path runs a consistency scan returning a count only. No sensitive content is exposed through the broadened gate.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 4: `maintain` parameter removal — backward compatibility

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:188-200` (StatusParams struct)
- **Description**: The `maintain` field is removed from `StatusParams`. Existing callers who pass `maintain: true` will have the field silently ignored at deserialization (serde does not set `deny_unknown_fields` on this struct). The parameter was already silently ignored since col-013; the struct removal merely cleans up the dead field. No mutation path is exposed or removed from the MCP call path. `run_maintenance` remains callable only from the background tick (`background.rs:309`), which is internal and not accessible via MCP tools.
- **Recommendation**: No action required.
- **Blocking**: no

## Blast Radius Assessment

Worst case: the gate change contains a logic bug in `require_cap` (e.g., the async `spawn_blocking` wrapper silently drops errors). In that scenario, `context_status` would return internal health metrics to any caller. The data returned is aggregate counts and coherence scores — no raw entry content, no embeddings, no agent credentials. The failure mode is information disclosure of internal metrics, not data corruption or privilege escalation. This is bounded and recoverable.

The `maintain` removal has no blast radius beyond removing a dead code path. `run_maintenance` continues to be called by the background tick exclusively; the MCP call path never invoked it after col-013.

## Regression Risk

Low. The four changed files are:

1. `infra/coherence.rs` — string literal change in recommendation messages only.
2. `infra/registry.rs` — new tests added, no logic changes.
3. `infra/validation.rs` — `maintain: None` fields removed from three test structs; the `StatusParams` struct no longer has this field so the tests compile cleanly.
4. `mcp/tools.rs` — single `Capability::Admin` → `Capability::Read` change; `maintain` field removed from struct; dead comment removed; two tests added.
5. `server.rs` — doc comment update only.
6. `services/status.rs` — doc comment update only.

No logic in the `compute_report` path was changed. Existing tests for `validate_status_params` still exercise topic and category validation. The Admin-gated mutation tools (`context_quarantine` at line 929, `context_enroll` at line 1035) are untouched and continue to require Admin.

## Dependency Safety

No new dependencies introduced. No `Cargo.toml` changes in the diff.

## Secrets Check

No hardcoded secrets, API keys, tokens, or credentials in the diff.

## PR Comments

- Posted 1 comment on PR #254.
- Blocking findings: no.

## Knowledge Stewardship

- nothing novel to store -- the gate-lowering pattern (Admin -> Read for read-only tools) is a one-time correction, not a recurring anti-pattern worth generalising.
