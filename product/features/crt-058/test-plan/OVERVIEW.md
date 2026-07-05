# Test Strategy — crt-058: Eager Agent-Authored Edge Cleanup at `context_deprecate`

## Overall Strategy

Three layers, all state/parse-based (never call-count or bare string-presence — SR-04, #5427):

1. **Unit (Rust, `cargo test --workspace`)** — the eager helper predicate behavior (both-direction, per-source, self-loop, high-degree, zero-row tolerance, atomic single-statement RETURNING), the formatter per-format matrix (`Some(n)`/`Some(0)`/`None`), and quarantine/restore backward-compat.
2. **Integration (Rust, in-process store)** — the `context_deprecate` handler step-6.5 orchestration end-to-end: flip → delete → count → audit → format, the idempotency/ordering guards, the non-fatal path, and the **AC-10 subset test invoking BOTH real functions** (`delete_agent_edges_for_entry` + `run_orphaned_edge_compaction`).
3. **Integration (Python, infra-001 MCP harness)** — the deprecate tool exercised through the JSON-RPC surface: `edges_removed` advisory in each format, audit record content, synchronous absence on return.

The load-bearing property is **eager ⊆ tick**; the delete is irreversible. Every high-priority risk gets a behavioral assertion against real code, not prose.

## Test Placement (cumulative — extend, never scaffold)

| Layer | File | Rationale |
|-------|------|-----------|
| Helper DB unit tests | new `edge_write_delete_agent_tests.rs` via `#[cfg(test)] #[path=...] mod` in `edge_write.rs` (existing inline `mod tests` at `:420` is pure-constant; keep it, add the DB module) | DB tests need a store fixture; follows the split-module pattern (#4904) |
| Subset test (AC-10) + handler orchestration | extend `background.rs` `mod tests` (`:1911`) — it already owns `insert_graph_edge_with_source`, `deprecate_entry_with_successor`, `run_orphaned_edge_compaction`, `total_graph_edges`, `count_graph_edges` | AC-10 needs the real compaction + its seed helpers side-by-side with the eager helper; reuse, do not duplicate |
| Formatter matrix | extend `mcp/response/mod.rs` `mod tests` (`:209`, already has `format_deprecate_success` cases) | formatter unit tests already live here |
| Audit read-back | pattern from `server.rs:2205` (`SELECT operation, target_ids, detail, metadata FROM audit_log WHERE operation = ?1`) — add `metadata` to the projection | reuse the established audit-readback pattern |
| MCP surface | infra-001 `suites/test_tools.py`, `suites/test_lifecycle.py` | deprecate already exercised there |

**Shared-helper note:** `insert_graph_edge_with_source` (`background.rs` test mod, private) is the canonical per-source seeding helper. The eager-helper DB tests and the subset test MUST seed from ONE shared helper (R-02 fixture-identity). If the edge_write DB module cannot reach it, promote it to a `pub(crate)` test helper rather than copying — a copy can drift and defeat the fixture-identity assertion.

## Risk-to-Test Mapping (from RISK-TEST-STRATEGY.md)

| Risk | Pri | Covering test(s) | Component plan |
|------|-----|------------------|----------------|
| R-01 subset blind spot (successor-less fixtures never hit the break case) | Critical | `test_deprecate_subset_R_subseteq_T_and_exactly_agent`, `test_correct_successor_never_invokes_eager_helper` (chokepoint-exclusion, real handler) | deprecate-handler |
| R-02 predicate drift | High | subset test invokes BOTH real fns; `test_eager_predicate_string_pinned`; fixture-identity assert | deprecate-handler, eager-delete-helper |
| R-03 post-commit atomicity | High | `test_delete_returning_single_statement_atomic`; `test_eager_err_leaves_edges_present_or_audited` | eager-delete-helper, audit-emit |
| R-04 zero-case (RESOLVED: `Some(0)`→`0`, ADR-004) | High | `test_format_edges_removed_some_zero_renders_literal_0_all_formats`; AC-05 handler test | response-formatter |
| R-05 per-format count drop | High | per-format matrix parsing Json integer + rendered Summary/Markdown values; quarantine/restore byte-identical | response-formatter |
| R-06 unguarded helper / single-caller | High | `test_single_production_caller_of_delete_agent_edges_for_entry` (callgraph/grep); R-01 chokepoint-exclusion doubles as misuse guard | deprecate-handler |
| R-07 concurrent-tick under-report | Med | `test_eager_tolerates_zero_row_returning` (already-swept → `Some(0)`, no audit, no panic) | eager-delete-helper |
| R-08 double audit-event confusion | Med | audit asserts filter `operation == "context_deprecate.edge_cleanup"`; `test_flip_and_cleanup_are_two_distinct_records` | audit-emit |
| R-09 provenance enumeration drift | Med | per-source matrix over full enum; only `agent` removed | eager-delete-helper |
| R-10 self-loop / high-degree | Low | `test_self_loop_agent_edge_removed_counted_once`; `test_high_degree_all_removed_audit_tuples_match` | eager-delete-helper, audit-emit |
| R-11 idempotency / ordering | High | `test_redeprecate_no_delete_no_cleanup_audit`; `test_edges_absent_synchronously_on_return`; flip-precedes-delete | deprecate-handler |

## AC-to-Component Coverage

| AC | Component plan | Key assertion |
|----|----------------|---------------|
| AC-01 both-direction removal | eager-delete-helper, deprecate-handler | inbound+outbound `source='agent'` gone on return |
| AC-02 inline count | response-formatter | subsumed by AC-04 per-format |
| AC-03 audit content | audit-emit | entry id, count in `detail`, tuples in `metadata` |
| AC-04 per-source + per-format | eager-delete-helper (source), response-formatter (format) | only `agent` removed; count value = N each format |
| AC-05 `Some(0)`→`0` | response-formatter | literal `0` all formats (distinct from AC-06 omit) |
| AC-06 non-fatal | deprecate-handler | success, `warn`+id, `None` omits, tick backstops |
| AC-07 idempotency | deprecate-handler | 2nd call early-returns, no delete, no cleanup audit |
| AC-08 no new persistence | eager-delete-helper (grep), deprecate-handler | schema/migration/compaction unchanged; `write_pool_server()` |
| AC-09 synchronous | deprecate-handler | edges gone immediately on return, no sleep |
| AC-10 subset invariant | deprecate-handler | R⊆T AND R==2 agent edges + chokepoint-exclusion + pinned predicate |
| AC-11 tuple audit | audit-emit | set-equality of N tuples in `metadata`, well-formed JSON, not `"{}"` sentinel |

Delivery-time closure items (ACCEPTANCE-MAP §Delivery-time): single-caller (R-06 → deprecate-handler), post-commit atomicity (R-03 → eager-delete-helper), distinct audit events (R-08 → audit-emit), self-loop counted once (R-10 → eager-delete-helper), quarantine/restore backward-compat (R-05 → response-formatter), concurrent-tick tolerance (R-07 → eager-delete-helper).

## Integration Harness Plan (infra-001)

**Suite selection** (feature touches: a server tool, store/retrieval, schema-adjacent graph writes, audit):

| Suite | Run | Why |
|-------|-----|-----|
| `smoke` | MANDATORY gate | minimum bar |
| `tools`, `protocol` | yes | deprecate tool logic + response shape |
| `lifecycle` | yes | store→deprecate→audit chain, restart persistence of the deprecated state |
| `edge_cases` | yes | zero-edge, self-loop, high-degree deprecation through the wire |
| `volume` | selective | high-degree entry deprecation at scale (audit-JSON growth, NFR-03) |
| `security` | selective | confirm agent-supplied `entry_id` is the only tainted input; no new injection surface |

**Existing coverage:** `test_tools.py` and `test_lifecycle.py` already exercise `context_deprecate` (the flip + idempotency). These validate the pre-feature success shape is preserved (NFR-04 backward compat).

**Gaps → new integration tests (Stage 3c to implement):**
- `test_tools.py::test_deprecate_reports_edges_removed_count` — store two entries, add an agent edge between them via `context_edge`, deprecate one, assert the response advisory carries the count (Json field parsed as integer, not substring).
- `test_tools.py::test_deprecate_zero_agent_edges_renders_literal_0` — deprecate an entry with no agent edges; assert advisory renders `0` (AC-05 through the wire), NOT omitted.
- `test_lifecycle.py::test_deprecate_removes_agent_edges_and_audits` — full chain: create edge → deprecate → assert edge no longer returned by `context_get`/graph read → assert the `edge_cleanup` audit is recorded.
- `test_edge_cases.py::test_deprecate_entry_with_no_edges_succeeds` — advisory `0`, success unchanged.

**Key integration concern (AC-10):** the subset test invoking the real `run_orphaned_edge_compaction` (`background.rs:805`, UNCHANGED) alongside the real eager helper is a **Rust in-process test**, not a Python MCP test — the compaction is not tool-invocable and both predicates must be exercised in the same process against parallel fixtures. It lives in the `background.rs` test module. This is the single most important integration assertion in the feature; call it out to the delivery tester explicitly.

**Do NOT plan new integration tests for:** internal atomicity/marshaling (R-03, unit-level fault injection), single-caller invariant (R-06, callgraph/grep), predicate-string pinning (R-02, Rust unit). These have no distinct MCP-visible effect beyond what the above cover.

## Failure Triage (Stage 3c)

Per USAGE-PROTOCOL.md decision tree: feature-caused → fix + re-run; pre-existing/unrelated → GH Issue + `@pytest.mark.xfail(reason="Pre-existing: GH#NNN")`, do not fix in this PR; bad assertion → fix test + document. Never delete or comment out an integration test.
