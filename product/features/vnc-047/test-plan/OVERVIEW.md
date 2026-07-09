# vnc-047 Test Plan — OVERVIEW

> `context_cycle` whole-set-once opaque run-identity `tags` → `cycle_tags` junction → surfaced in
> `context_cycle_review`. Tracks GH #940. Rooted in RISK-TEST-STRATEGY.md (R-01…R-16) and
> ACCEPTANCE-MAP.md (AC-01…AC-09, AC-EXTRA-1…4). Component boundaries mirror the architecture
> Component Map (C1–C13).

## 1. Overall Test Strategy

Three test tiers, in decreasing proximity to production wiring:

1. **Assembled-path Rust integration tests** (highest value — gate-critical for AC-02/AC-05).
   Driven **in-module** in `crates/unimatrix-server/src/uds/listener.rs` via
   `dispatch_request(HookRequest::RecordEvent { event }, …)` — the real hook→listener→store seam.
   Model: the existing col-025/GH#389 test `test_cycle_start_goal_flows_from_hook_payload_to_db`
   (`listener.rs:7934`, T-389-02): construct store+registry with the existing `make_store()`/
   `make_registry()`/`make_dispatch_deps()`/`make_services()`/`make_cycle_event()` helpers, fire a
   `RecordEvent`, `sleep(100ms)` for the fire-and-forget spawn, then read back via
   `store.get_cycle_tags(fc)`. This chain is the ONLY tier that proves tags actually flow from a
   cycle-start event into `cycle_tags` — a store-only insert/get test is NOT acceptable proof for
   AC-02/AC-05 (R-03/SR-08; the #917/#918/#930 green-test-holed-capability family).
2. **Store + unit tests** (`unimatrix-store`, `unimatrix-observe`, `unimatrix-server` seam fns).
   Migration cascades, the write primitive's atomicity/whole-set-once/opacity logic, the read
   getter, serde round-trip + backward-read, markdown render, GC regression. These SUPPLEMENT but do
   not SATISFY the assembled ACs.
3. **infra-001 Python integration harness** (`product/test/infra-001`). Drives the compiled binary
   over MCP JSON-RPC — the **bare** `context_cycle` handler, which by design persists NOTHING for
   tags (Constraint: hook-path only). See §5 for exactly what this tier can and cannot prove.

**Test conventions:** Rust `#[tokio::test]` for async, Arrange/Act/Assert, deterministic (no wall-clock
races beyond the established 100ms fire-and-forget settle used by T-389-02). Naming
`test_{concept}_{scenario}_{expected}`. Extend existing test modules — do NOT create isolated
scaffolding (NFR-9; test infra is cumulative).

## 2. The critical seam nuance (read before Stage 3c)

The infra-001 Python harness cannot exercise the tag-persistence path. Tags ride
`tool_input["tags"]` through the **UDS hook** (`build_cycle_event_or_fallthrough` → `RecordEvent` →
`handle_cycle_event`); the bare MCP `context_cycle` handler is session-unaware and persists nothing
(tools.rs:4062). Therefore:

- **Positive persistence + surfacing (AC-02, AC-02a/b, AC-05, AC-EXTRA-2/3)** → **Rust assembled
  tests in `listener.rs`** via `dispatch_request`. `proven_by` MUST cite these, never a store getter.
- **Negative "no second route" (AC-EXTRA-1)** → Python harness CAN prove this: a bare
  `context_cycle(start, tags=[…])` over MCP followed by `context_cycle_review` shows NO tags. Plus a
  Rust/grep assertion that `cycle_tags` INSERT appears only in `insert_cycle_start_with_tags`.
- **AC-09 ack echo (non-gating)** → Python harness string-assertion on the ack is the cheapest
  observation point.

## 3. Risk → Test Mapping

| Risk | Pri | Component plan | Representative test(s) | Tier |
|------|-----|----------------|------------------------|------|
| R-01 schema v31 cascade | Critical | cycle_tags-migration | `test_migration_v30_to_v31_creates_cycle_tags`, `test_current_schema_version_is_at_least_31`, fresh-create table+index assert | store/migration |
| R-02 SUMMARY v6 cascade | Critical | report-field | `test_summary_schema_version_is_6`, `test_retrospective_report_tags_roundtrip`, `test_v5_blob_deserializes_tags_default_empty` | unit (observe/store) |
| R-03 assembled path only structural | Critical | listener-persistence, review-handler | `test_cycle_start_tags_flow_from_hook_to_cycle_tags`, `test_review_surfaces_tags_json_and_markdown_assembled` | **assembled** |
| R-04 silent drop absent session | High | listener-persistence | `test_evicted_session_tags_persist`, `test_empty_feature_cycle_no_orphan_rows`, `test_get_cycle_tags_read_error_degrades_review` | **assembled** |
| R-05 txn not atomic | High | store-write-primitive | `test_start_row_and_tag_rows_share_commit`, `test_dup_tag_in_set_no_txn_abort`, fault-inject or code-review sign-off | store |
| R-06 second persistence route | High | listener-persistence, deferred-seam | `test_bare_mcp_handler_persists_no_tags` (Python + Rust), grep single-writer | Python + grep |
| R-07 GC purge | High | gc-protection | `test_gc_protected_tables_regression` extended: cycle_tags across BOTH surfaces + `sessions` positive control | store |
| R-08 whole-set-once reads as loss | Med | store-write-primitive, listener-persistence | `test_whole_set_once_changed_set_noop`, `_superset_noop`, `_subset_noop`, `test_tagless_start_does_not_lock` (EXACT set equality) | **assembled** + store |
| R-09 15 call sites regress | Med | listener-persistence | `test_start_without_tags_routes_to_insert_cycle_event`, `test_non_start_event_unchanged`, `test_goal_persists_with_and_without_tags` | **assembled** |
| R-10 version collision | Med | (OVERVIEW checklist) | SR-02 re-verify at impl start, coverage-report line item | manual |
| R-11 empty-tag / opacity leak | Med | store-write-primitive, hook-extraction | `test_empty_string_tag_rejected_others_stored`, `test_colon_and_bare_stored_verbatim_no_derivation` | store/unit |
| R-12 markdown render divergence | Low | markdown-render | `test_render_tags_section_present`, `test_render_no_spurious_section_when_empty` (assert vs SPEC/render_goal_section parity, NOT ARCHITECTURE header string, #3337) | unit |
| R-13 migration not idempotent/old-DB | Med | cycle_tags-migration | `test_migration_v30_to_v31_idempotent`, `test_migration_from_populated_v30_data_intact` | store/migration |
| R-14 no back-fill misread | Low | report-field | `test_v5_cached_review_shows_no_tags` (optional confirm) + doc | unit |
| R-15 EXISTS-guard TOCTOU | High | store-write-primitive | `test_begin_immediate_used` (review/assert), `test_concurrent_same_cycle_starts_one_whole_set` | store (concurrency) |
| R-16 ack echo drift | Low **NON-GATING** | ack-echo, freeze-trace | ack string assert; listener trace log-assert. MUST NOT block a gate | unit/Python |

## 4. Acceptance Criteria → Test Mapping

| AC | Gating | Proven by | Plan |
|----|--------|-----------|------|
| AC-01 opacity | GATING | store `test_empty_string_tag_rejected_others_stored`, `test_stored_verbatim_no_truncation` + assembled opacity | store-write-primitive, hook-extraction |
| AC-02 whole-set-once via hook | GATING **[assembled]** | `test_cycle_start_tags_flow_from_hook_to_cycle_tags`, `test_non_start_tags_not_persisted`, `test_duplicate_start_no_dup_no_error` | listener-persistence |
| AC-02a whole-set no-op | GATING **[assembled]** | `_changed_set_noop`, `_superset_noop`, `test_tagless_start_does_not_lock` (EXACT equality) | listener-persistence, store-write-primitive |
| AC-02b concurrency | GATING | `test_begin_immediate_used` (review), `test_concurrent_same_cycle_starts_one_whole_set` | store-write-primitive |
| AC-03/a-d schema v31 | GATING | const grep + fresh-create + migration + idempotent + pinned + DDL-parity | cycle_tags-migration |
| AC-04 GC omission | GATING | extended `test_gc_protected_tables_regression` (both surfaces + positive control) | gc-protection |
| AC-05/a-d SUMMARY v6 + surface | GATING **[assembled]** | const grep + serde round-trip + backward-read + pinned + `test_review_surfaces_tags_json_and_markdown_assembled` | report-field, review-handler, markdown-render |
| AC-06 interface + authz | GATING | `test_context_cycle_requires_write_cap`, `test_agent_id_not_authorizing`, no-new-tool (capability-classification anchor), `context_tag`/`context_correct` diff-clean | cycle-params, deferred-seam |
| AC-07 prefix not enforced | GATING | `test_colon_and_bare_stored_identically_no_branching` | store-write-primitive |
| AC-08 no back-fill | GATING (doc) | doc + optional `test_v5_cached_review_shows_no_tags` | report-field |
| AC-09 ack echo | **NON-GATING** | ack string assert (best-effort) | ack-echo, freeze-trace |
| AC-EXTRA-1 no 2nd route | GATING | `test_bare_mcp_handler_persists_no_tags` (Python) + grep single-writer | listener-persistence |
| AC-EXTRA-2 absent session | GATING **[assembled]** | `test_evicted_session_tags_persist`, `test_empty_feature_cycle_no_orphan_rows`, degrade | listener-persistence |
| AC-EXTRA-3 txn atomicity | GATING | `test_start_row_and_tag_rows_share_commit`, `test_dup_tag_in_set_no_txn_abort` | store-write-primitive |
| AC-EXTRA-4 SR-02 re-verify | GATING | coverage-report checklist (see §6) | manual |

## 5. Integration Harness Plan (infra-001)

**Suite selection** (per USAGE-PROTOCOL suite table — feature touches server tool logic + schema/
storage + review surfacing):

| Suite | Run? | Why |
|-------|------|-----|
| `smoke` | **MANDATORY gate** | minimum per USAGE-PROTOCOL |
| `tools` | Yes | additive `tags` param on `context_cycle` must not break the handler; all response formats intact |
| `protocol` | Yes | handshake/tool-discovery unchanged; "no new tool" surface stable |
| `lifecycle` | Yes | restart persistence + `context_cycle`→`context_cycle_review` flow; confirms review still renders |
| `edge_cases`, `volume` | Recommended | schema change → restart persistence + scale sanity (non-blocking if irrelevant) |
| `security` | Yes (light) | capability enforcement on `context_cycle` (Write) intact |

**Coverage of feature behavior by existing suites:** existing suites exercise `context_cycle` and
`context_cycle_review` generically; NONE exercise the tag persistence path (it is hook-only, and the
Python harness drives the bare MCP boundary — §2). Existing suites therefore cover **regression
safety** (additive param doesn't break the tool) but NOT the new positive behavior.

**Gaps → new tests to add in Stage 3c:**
- New Python `suites/test_tools.py`: `test_context_cycle_accepts_tags_param` — bare
  `context_cycle(start, tags=[…])` returns success (additive param accepted, interface stable).
- New Python `suites/test_lifecycle.py`: `test_bare_mcp_cycle_tags_not_persisted` — bare start with
  tags then `context_cycle_review` shows NO tags (proves AC-EXTRA-1 no-second-route from the MCP
  boundary). This is the ONE positive statement the Python harness can make about the constraint.
- New Python `suites/test_tools.py` (best-effort, non-gating): `test_context_cycle_ack_echoes_tags`
  — ack string contains the accept-for-recording note on a start-with-tags call. Do NOT gate on it.

**Do NOT add to Python harness:** the positive tag-persistence and review-surfacing tests — those
are Rust assembled tests (§2). No harness infrastructure changes are needed (no GH issue required).

## 6. Cross-Component Test Dependencies & Checklist

- C4 hook → C5 listener payload contract: `payload["tags"]` JSON array parity with `payload["goal"]`
  (hook.rs:877-880). A missing/mistyped key must degrade to "no tags", never panic — asserted in
  both hook-extraction and listener-persistence plans.
- C5 listener → C2 store: the Start-with-tags-vs-else branch is the single routing decision keeping
  the (verified 8-arg) `insert_cycle_event` 15 call sites untouched (R-09).
- C2 write primitive is BOTH the atomicity seam (R-05) and the concurrency-safety seam (R-15).
- C3 getter → C8 review handler → C9 render → C7 field serde: the read/surface chain; the review
  handler MUST read fresh from `cycle_tags` (source of truth), never trust a stale `summary_json`
  mirror.

**SR-02 / AC-EXTRA-4 re-verification checklist (record verbatim in RISK-COVERAGE-REPORT.md at Stage
3c start):** confirm at HEAD `CURRENT_SCHEMA_VERSION == 30` (`migration.rs:26`) and
`SUMMARY_SCHEMA_VERSION == 5` (`cycle_review_index.rs:54`) are still the next-free numbers before
writing bumps to 31 / 6. Flag renumber if a parallel merge claimed either. (Confirmed free at
synthesis 2026-07-09.)

## 7. Open Questions (for delivery)

1. **`insert_cycle_event` signature — RESOLVED (matches pseudocode + HEAD).** HEAD `insert_cycle_event`
   (db.rs:320) is 8-arg `(cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal)`.
   `goal_embedding` is NOT written by this INSERT — it stays NULL and is populated later by the
   existing `update_cycle_start_goal_embedding` UPDATE (Step-6) on `event_type='cycle_start'`. The new
   `insert_cycle_start_with_tags` mirrors this exactly (`event_type` fixed to `'cycle_start'`,
   `goal_embedding` left NULL). Tests assert the produced start row is byte-identical to what the
   plain `insert_cycle_event` path writes. (An earlier tester note claiming "no next_phase, carries
   goal_embedding" was incorrect and is superseded by this reconciliation.)
2. **Review handler rmcp seam (R-03/#5389).** `context_cycle_review` is an rmcp `#[tool]` handler;
   `RequestContext<RoleServer>` cannot be constructed in unit scope. The tag-populate logic
   (`report.tags = get_cycle_tags(fc).await.unwrap_or_default()`) MUST be reachable via an extracted
   `pub(crate)` seam so the assembled test drives real `get_cycle_tags` + real render, not a
   hand-built report asserting its own literal. Flag if the handler is not seam-extractable.
