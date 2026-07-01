# Security Review: vnc-042-security-reviewer

## Risk Level: low

## Summary
vnc-042 makes `context_get` resolve a deprecated id to its active terminal by default. The change is a read-only, additive contract change confined to the MCP server crate. No new attack surface, no new data reachability, no dependency/secret changes. The reused 50-hop supersession primitive is intact. No blocking findings.

## Findings

### INFO-1: Tool-description const not byte-bound to the live literal
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs` (`CONTEXT_GET_DESCRIPTION`)
- **Description**: The const is `#[allow(dead_code)]` and `test_get_tool_description_documents_follow_supersessions` asserts only substring presence, not byte-equality with the `#[tool(description = ...)]` literal on `context_get`. The two can drift silently (tool-description-lies hazard #4303). They match in this diff. The const's doc-comment is a copy-paste leftover still referencing `context_graph`.
- **Recommendation**: Add a byte-equality assertion binding the const to the live literal (as done for `context_graph`), or drop the unused const. Fix the doc-comment reference.
- **Blocking**: no

### R-09: Default-on resolution changes what durable-id callers receive (accepted)
- **Severity**: low (accepted product bet, LOCKED #843)
- **Location**: `context_get` handler
- **Description**: Non-code consumers (memory files, agent/skill defs, prior-session ids) that intentionally passed a deprecated id now receive the terminal by default. No new data is disclosed — the terminal is directly gettable by id anyway.
- **Recommendation**: None; discoverability proxy is the tool-description update (present).
- **Blocking**: no

### NG-1: Mixed-resolution edge targets (accepted)
- **Severity**: low
- **Location**: formatter / edge assembly
- **Description**: A resolved get returns the terminal's edge list but edge targets remain unresolved (old id+title). Resolution notice makes it legible.
- **Recommendation**: Deferred follow-up (neighbor-target resolution). Not security.
- **Blocking**: no

## OWASP Assessment
- **Injection**: none. `follow_supersessions` is plain `Option<bool>`, no string coercion (quoted scalar rejects). Store reads parameterized.
- **Access control / info disclosure**: no new reachability. Pre-vnc-042 handler was a raw `entry_store.get(id)` with no status filter, so deprecated/quarantined entries were already returnable by exact id. Resolution only changes which already-authorized entry is returned. Default-follow returns `Some` only on `status == Active`; quarantined/orphaned terminals collapse to a loud dead-end — never surfaced as "current". Mild improvement.
- **DoS**: 50-hop cap + active-terminal guard in `follow_to_current` (`graph_read_neighbors.rs:36-55`) intact and untouched. Cycle/self-loop trips the cap. Bounded traversal.
- **Deserialization**: additive `Option<bool>` only; no untrusted structured payload, no `unsafe`.
- **Error handling / fail-loud**: store error in `follow_to_current` → `None` → loud `DeadEnd`. Primary fetch + `build_edges_view` failures map to `ServerError::Core` and return (unchanged). No `.unwrap()` in production path; `superseded_by: None` matched, not unwrapped — no `#null`/panic.
- **Secrets**: none. **Dependencies**: no `Cargo.toml`/lock change.

## Blast Radius Assessment
`context_get` is the most-used read tool and this flips its default. Worst case of a subtle bug: returning the wrong entry, or the terminal's content paired with the requested id's edges. Mitigated — `effective_id` has a single source (`resolve_effective_id`) threaded to **both** the fetch and `build_edges_view` (verified in diff), and clean passthrough routes to the unmodified `format_single_entry` preserving byte-identity. The tool is read-only: no write path, so the failure mode is a returned error or a visibly-flagged result, never silent data corruption or privilege escalation.

## Regression Risk
Low. Additive serde field; `resolution` key emitted only via `format_single_entry_with_note`, keeping the common active-entry JSON byte-identical (canary + shape tests unchanged). Audit/usage now account `effective_id` — intentional attribution shift to the entry actually returned; not a defect. Test-harness (`client.py`/`uds_client.py`) changes are additive and omit the field when absent.

## PR Comments
- Posted 1 review comment on PR #867 (state: COMMENTED).
- Blocking findings: no.

## Knowledge Stewardship
- Stored: nothing novel to store — the governing anti-patterns (serde-default footgun #3774/#3817, tool-descriptions-must-not-lie #4303, blast-radius partitioning #5383) already exist and are generalized; INFO-1 is a single-PR instance of #4303, not a new cross-feature pattern. Filed as PR comment per policy (bugs/PR-specific findings are not lessons).
