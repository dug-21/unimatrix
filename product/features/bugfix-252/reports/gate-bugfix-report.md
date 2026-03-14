# Gate Bug Fix Report: bugfix-252

> Gate: Bug Fix Validation
> Date: 2026-03-14
> Result: PASS (with WARN)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | `require_cap(Admin)` changed to `require_cap(Read)` at line 774 of tools.rs |
| No placeholders | PASS | No `todo!()`, `unimplemented!()`, TODO, FIXME in changed files |
| All tests pass | PASS | 2169 unit + integration tests pass; 5 new tests all pass |
| No new clippy warnings | PASS | Warnings in changed files are pre-existing; no new ones introduced |
| No unsafe code | PASS | No `unsafe` blocks in any changed file |
| Fix is minimal | PASS | 4 files changed, 95 insertions / 16 deletions, all scoped to the bug |
| New tests catch original bug | PASS | `test_status_read_cap_non_admin_agent_lacks_admin` proves the old gate would have failed |
| Integration smoke tests | PASS | Full workspace test suite green |
| xfail markers | PASS | No `#[ignore]` markers added |
| Knowledge stewardship — investigator | PASS | Queried #317, #1369; stored #1435 |
| Knowledge stewardship — rust-dev | PASS | Queried #317, #1369; nothing novel stored with reason documented |
| Knowledge stewardship — tester | WARN | Store attempt blocked by server error; explanation documented but no stored entry |
| Stale user-facing strings | WARN | `coherence.rs` lines 184, 191 still reference "maintain: true" in recommendation messages |

## Detailed Findings

### Root Cause Addressed
**Status**: PASS
**Evidence**: `mcp/tools.rs:774` — `self.require_cap(&ctx.agent_id, Capability::Read).await?;` (was `Capability::Admin`). Tool description at line 764 updated to "Requires Read capability." The `maintain: Option<bool>` field is gone from `StatusParams` (lines 189–200 show the struct with 5 fields, no `maintain`). The handler body no longer references `maintain` or calls `run_maintenance` directly.

### No Placeholders
**Status**: PASS
**Evidence**: grep for `todo!()`, `unimplemented!()`, TODO, FIXME across all four changed files returns no results.

### All Tests Pass
**Status**: PASS
**Evidence**: `cargo test --workspace` passes all test suites. Breakdown:
- unimatrix-store: 1185 passed
- unimatrix-server: 353 passed (lib) + multiple integration suites
- embed, vector, core, observe: all green
- 5 new bugfix/252 tests confirmed individually: `test_status_params_no_maintain_field`, `test_status_params_anonymous_agent_deserializes`, `test_status_read_cap_non_admin_agent_passes`, `test_status_read_cap_non_admin_agent_lacks_admin`, `test_status_anonymous_fresh_install_passes_read_gate` — all pass.

### No New Clippy Warnings
**Status**: PASS
**Evidence**: Warnings present in changed files (`tools.rs:373`, `tools.rs:1331`, `tools.rs:1395`, `server.rs`) are pre-existing patterns (fire-and-forget spawn_blocking, async closure patterns) unrelated to the fix. No warnings appear in the modified code regions (lines 189–200 for StatusParams, lines 762–827 for the handler).

### No Unsafe Code
**Status**: PASS
**Evidence**: grep for `unsafe` in all four changed files returns no results.

### Fix Is Minimal
**Status**: PASS
**Evidence**: `git show --stat HEAD` shows exactly 4 files: `infra/registry.rs` (+63 lines of tests), `infra/validation.rs` (-3 lines removing `maintain: None` from test fixtures), `mcp/tools.rs` (+41/-15: StatusParams field removal + capability change + comment cleanup + new tests), `server.rs` (+2/-2: comment cleanup). No unrelated changes included.

### New Tests Catch Original Bug
**Status**: PASS
**Evidence**: `test_status_read_cap_non_admin_agent_lacks_admin` (registry.rs:922) explicitly asserts that a Restricted agent fails an Admin capability check, directly proving the old gate would have blocked them. `test_status_read_cap_non_admin_agent_passes` proves the new gate (Read) lets them through. `test_status_params_no_maintain_field` confirms the struct no longer has the field. `test_status_anonymous_fresh_install_passes_read_gate` covers the fresh-install scenario.

### Integration Smoke Tests
**Status**: PASS
**Evidence**: `cargo test --workspace` — all suites green, including migration integration and export/import integration tests.

### xfail Markers
**Status**: PASS
**Evidence**: No `#[ignore]` annotations added in the commit diff.

### Knowledge Stewardship — Investigator
**Status**: PASS
**Evidence**: Queried entries #317 and #1369; stored entry #1435. Fulfilled stewardship obligations.

### Knowledge Stewardship — Rust-Dev
**Status**: PASS
**Evidence**: Queried entries #317 and #1369; "nothing novel stored" — pattern was already captured by investigator's #1435. Valid reason documented.

### Knowledge Stewardship — Tester
**Status**: WARN
**Evidence**: Queried procedures. Store attempt was blocked by server error, preventing storage. The failure is documented in the spawn prompt ("store attempt blocked by server error"). The inability to store is an infrastructure failure, not a compliance failure — the intent was present. This is acceptable but worth noting.

### Stale User-Facing Strings
**Status**: WARN
**Evidence**: `infra/coherence.rs` lines 184 and 191 contain user-visible recommendation strings:
- `"... -- run with maintain: true to refresh"`
- `"... -- run with maintain: true to compact"`

These advise users to pass a `maintain` parameter that no longer exists in `StatusParams`. The fix scope listed `mcp/tools.rs`, `infra/validation.rs`, `infra/registry.rs`, `server.rs` — `coherence.rs` and `mcp/response/mod.rs` (line 1426–1427, test fixture data) were not included. The recommendation strings are misleading but do not break functionality. The background tick still runs maintenance; the strings are advisory only. Tracked as follow-up issue recommended.

## Rework Required

None. Both WARNs are minor gaps that do not block functionality or correctness.

## Recommended Follow-Up (Not Blocking)

| Issue | File | Fix |
|-------|------|-----|
| Stale "maintain: true" recommendation strings | `infra/coherence.rs:184,191` | Update to suggest using `context_status` with `check_embeddings: true` or note maintenance is automatic |
| Same in test fixture | `mcp/response/mod.rs:1426-1427` | Update fixture strings to match updated coherence.rs output |
| Stale doc comment | `services/status.rs:608` | Remove "matches maintain=true path in original handler" from `run_maintenance` doc |

## Knowledge Stewardship

- Stored: nothing novel to store -- gate result is feature-specific; no cross-feature pattern identified beyond what investigator stored in #1435.
