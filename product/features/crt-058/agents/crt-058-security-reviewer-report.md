# Security Review: crt-058-security-reviewer

## Risk Level: low

## Summary
The change adds a single parameterized `DELETE FROM graph_edges … RETURNING` at `context_deprecate` step 6.5, an `Option<u64>` `edges_removed` threaded through the shared status formatter, and a fire-and-forget `context_deprecate.edge_cleanup` audit event whose metadata is `serde_json` of the removed tuples. No injection, deserialization, path, secret, or dependency surface. Access is Write-capability-gated and bounded to the single deprecated entry. No blocking findings.

## Findings

### F-1: SQL predicate fully parameterized (no injection)
- **Severity**: informational (verified clean)
- **Location**: `crates/unimatrix-server/src/mcp/edge_write.rs:345-355`
- **Description**: `WHERE (source_id = ?1 OR target_id = ?1) AND source = ?2 RETURNING …`. `?1` = `entry_id as i64` (range-validated by `validate_deprecate_params` before binding); `?2` = `EDGE_SOURCE_AGENT` compile-time constant, not user input. `RETURNING` columns are literal. No user string (relation_type, reason) enters the SQL. No interpolation.
- **Recommendation**: none.
- **Blocking**: no.

### F-2: Access control — gated and bounded
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:1428` (`require_cap(Write)`), delete predicate keyed on single `entry_id`
- **Description**: The handler enforces `require_cap(Capability::Write)` before reaching the delete. The predicate is bounded to edges touching one entry. The eager path adds no reach beyond the pre-existing `EveryTick` compaction (all it changes is latency: ≤900s → synchronous). No new authorization surface introduced.
- **Recommendation**: none.
- **Blocking**: no.

### F-3: Audit metadata serialization is injection-safe
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/server.rs:690-706` (`emit_edge_cleanup_audit`)
- **Description**: Removed tuples are serialized via `serde_json::to_string(removed)` — `relation_type` strings are encoder-escaped, no string interpolation. `detail` uses `format!` over integers only (count, entry_id). On a serialization error the event is skipped with a `warn`, rather than emitting the audit-layer `"{}"` empty-metadata sentinel alongside a non-empty removal. No format-string or metadata-corruption surface.
- **Recommendation**: none.
- **Blocking**: no.

### F-4: Non-fatal path is atomic — no committed-delete-without-audit gap
- **Severity**: informational (RISK-TEST-STRATEGY R-03 closed)
- **Location**: `crates/unimatrix-server/src/mcp/edge_write.rs:348-368`
- **Description**: The delete and tuple capture are one `DELETE … RETURNING` executed via a single `fetch_all`; there is no delete-then-separate-SELECT window. Row marshaling is in-memory typed extraction over columns that are NOT NULL in schema (`relation_type TEXT NOT NULL`, i64 ids at `db.rs:956`), so it cannot return `Err` after commit. "Edges gone ⟺ tuples captured ⟺ audit emitted" holds. On any `Err` from `fetch_all` (at/before commit) the failure is swallowed (`warn`, not `debug`), advisory omitted, and `run_orphaned_edge_compaction` backstops.
- **Recommendation**: see F-6 (residual panic path, defense-in-depth only).
- **Blocking**: no.

### F-5: Regression surface contained
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/mcp/response/mutations.rs`, `background.rs`
- **Description**: The `format_status_change` signature gains `edges_removed: Option<u64>` before `format`; `format_quarantine_success`/`format_restore_success` pass `None` and have byte-identical-output tests pinning no behavior change. The `insert_graph_edge_with_source` seed helper promoted to `pub(crate)` is `#[cfg(test)]` — test-only, absent from the shipping binary. `run_orphaned_edge_compaction` is unchanged (the +340 background.rs lines are entirely test code). No new dependencies (`Cargo.toml`/`Cargo.lock` untouched). No secrets.
- **Recommendation**: none.
- **Blocking**: no.

### F-6: Marshaling uses panicking `row.get` (defense-in-depth)
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/edge_write.rs:361-365`
- **Description**: The RETURNING rows are marshaled with the panicking `row.get::<T,_>` rather than `try_get`. Safe today because the RETURNING columns match schema types and are NOT NULL. If a future schema change makes `relation_type` nullable or alters a column type, this panics *after* the delete has committed — unwinding the handler past the already-committed flip, the one residual path around the "non-fatal" contract (a panic, not the swallowed `Err`).
- **Recommendation**: optional — switch to `try_get` mapped into `EdgeDeleteError` to keep the non-fatal guarantee airtight against schema drift. Low likelihood; not required for merge.
- **Blocking**: no.

## Blast Radius Assessment
Worst case of a subtly-wrong predicate: dropping the id filter would delete all agent edges table-wide; dropping the source filter would delete all edges on the entry (machine included). Both are caught by the `test_deprecate_eager_subset_of_tick_and_exactly_agent_edges` test, which asserts the removed set `R` equals *exactly* the two agent edges over both real functions — any widening fails the exact-set assertion, and any set beyond the entry fails `R ⊆ T`. The eager⊆tick invariant is therefore a real, executable safety bound rather than prose. The delete is irreversible but the removed tuples are captured in the audit metadata for reconstructability.

Operational blast radius: a Write-capable agent can deprecate a high-degree hub entry to force mass deletion of agent-authored edges touching it, now accelerated to the deprecation call instead of ≤900s later. This is the intended semantics of retiring the entry, bounded to edges touching the single id, and audited. Property, not a new vulnerability.

## Regression Risk
Low. Formatter ripple is additive with `None` at non-delete call sites and byte-identical-output tests; the production tick is untouched; the promoted seed helper is test-gated; no dependency or config change. The only cross-cutting behavior change is the new `edges_removed` advisory on `context_deprecate` responses, which is additive across all three formats.

## PR Comments
- Posted 1 review comment on PR #911 (`gh pr review 911 --comment`).
- Blocking findings: no.

## Knowledge Stewardship
- Stored: nothing novel to store — the relevant anti-patterns (multi-pass same-table filter divergence enforced by a behavioral test over both real predicates; fire-and-forget warn-not-debug discipline) are already captured as patterns/lessons #3910/#5417/#3448 and cited throughout the feature's ADRs and RISK-TEST-STRATEGY. The `try_get`-vs-`get` post-commit-panic nuance (F-6) is a per-call-site robustness note, not a generalizable security anti-pattern.
