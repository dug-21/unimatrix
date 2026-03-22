# crt-026: WA-2 Session Context Enrichment — Test Plan Overview

GH Issue: #341

---

## Test Strategy

crt-026 adds a seventh scoring dimension (histogram affinity) to the search pipeline and a
histogram summary to the UDS CompactPayload. The testing strategy has three layers:

**Layer 1 — Unit tests (per-component)**: Pure function and struct behavior. Each component has
its own test plan file. Tests run via `cargo test --workspace`. Seven gate-blocking tests must
pass before Gate 3c clears.

**Layer 2 — Integration tests (infra-001 harness)**: System-level validation through the MCP
JSON-RPC interface. Exercises the full pipeline including histogram accumulation across tool
calls. Details in the Integration Harness Plan section below.

**Layer 3 — Code review assertions**: AC-04 (struct field presence), AC-14 (no WA-2 stubs),
R-12 (struct literal sites updated), R-13 (pre-resolution before await) — verified by grep
and code review, not automated tests.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Test(s) | Component |
|---------|----------|---------|-----------|
| R-01 | Critical | `test_histogram_boost_score_delta_at_p1_equals_weight`, `test_60_percent_concentration_score_delta`, `test_absent_category_phase_histogram_norm_is_zero` | fused-score.md, search-handler.md |
| R-02 | High | `test_cold_start_search_produces_identical_scores`, `test_phase_histogram_norm_zero_when_category_histogram_none` | fused-score.md, search-handler.md |
| R-03 | High | `test_duplicate_store_does_not_increment_histogram` | store-handler.md |
| R-04 | Medium | `test_record_category_store_unregistered_session_is_noop`, `test_get_category_histogram_unregistered_returns_empty` | session.md |
| R-05 | High | `test_uds_search_path_histogram_pre_resolution` | uds.md |
| R-06 | High | `test_fusion_weights_effective_nli_absent_excludes_phase_from_denominator` | fused-score.md |
| R-07 | Medium | `test_phase_explicit_norm_placeholder_fields_present` | fused-score.md, config.md |
| R-08 | Medium | `test_status_penalty_applied_after_histogram_boost` | fused-score.md |
| R-09 | Medium | `test_phase_histogram_norm_zero_when_total_is_zero` | fused-score.md |
| R-10 | Medium | `test_compact_payload_histogram_block_present_and_absent` | uds.md |
| R-11 | Medium | `test_config_validation_rejects_out_of_range_phase_weights` | config.md |
| R-12 | Medium | Compilation gate (cargo build) + code review of struct literal sites | cross-cutting |
| R-13 | Medium | Code review: `get_category_histogram` call before first `await` in both handlers | cross-cutting |
| R-14 | Low | Code review: `grep "WA-2 extension" search.rs` returns zero matches | cross-cutting |

---

## Gate-Blocking Tests (7 Required)

All seven must pass before Gate 3c clears. The gate-blockers are:

| # | Test Name | File | AC | Risk |
|---|-----------|------|----|------|
| 1 | `test_histogram_boost_score_delta_at_p1_equals_weight` | `services/search.rs` | AC-12 | R-01 |
| 2 | `test_duplicate_store_does_not_increment_histogram` | `mcp/tools.rs` | AC-02 | R-03 |
| 3 | `test_cold_start_search_produces_identical_scores` | `services/search.rs` | AC-08 | R-02 |
| 4 | `test_record_category_store_unregistered_session_is_noop` | `infra/session.rs` | AC-03 | R-04 |
| 5 | `test_compact_payload_histogram_block_present_and_absent` | `uds/listener.rs` | AC-11 | R-10 |
| 6 | `test_absent_category_phase_histogram_norm_is_zero` | `services/search.rs` | AC-13 | R-01, R-13 |
| 7 | `test_fusion_weights_effective_nli_absent_excludes_phase_from_denominator` | `services/search.rs` | AC-08 | R-06 |

---

## Acceptance Criteria Verification Map

| AC-ID | Test(s) | Method |
|-------|---------|--------|
| AC-01 | `test_register_session_category_counts_empty` | Unit |
| AC-02 | `test_duplicate_store_does_not_increment_histogram` | Unit (gate) |
| AC-03 | `test_record_category_store_unregistered_session_is_noop` | Unit (gate) |
| AC-04 | `grep "session_id: Option<String>"` in `ServiceSearchParams` | Code review |
| AC-05 | `test_context_search_handler_populates_service_search_params` | Unit |
| AC-06 | Transitively via AC-12 | Unit |
| AC-07 | DROPPED — not in scope | N/A |
| AC-08 | `test_cold_start_search_produces_identical_scores` | Unit (gate) |
| AC-09 | `test_phase_explicit_norm_placeholder_fields_present` | Unit |
| AC-10 | `test_status_penalty_applied_after_histogram_boost` | Unit |
| AC-11 | `test_compact_payload_histogram_block_present_and_absent` | Unit (gate) |
| AC-12 | `test_histogram_boost_score_delta_at_p1_equals_weight` | Unit (gate) |
| AC-13 | `test_absent_category_phase_histogram_norm_is_zero` | Unit (gate) |
| AC-14 | `grep "WA-2 extension" services/search.rs` → zero results | Code review |

---

## Cross-Component Test Dependencies

1. `session.rs` tests are self-contained (`SessionRegistry` has no async dependencies).
2. `store-handler.md` tests depend on `SessionRegistry.record_category_store` from Component 1.
   The handler test constructs a `SessionRegistry` and verifies state after a simulated store.
3. `search-params.md` tests are structural (struct layout) — no runtime dependencies.
4. `search-handler.md` tests depend on `SessionRegistry.get_category_histogram` from Component 1.
5. `fused-score.md` tests are pure function tests — no session or store dependencies.
6. `config.md` tests are pure struct tests — no server dependencies.
7. `uds.md` tests depend on `SessionRegistry.get_category_histogram` from Component 1.

The dependency graph is acyclic: session → (store-handler, search-handler, uds) → fused-score.

---

## Integration Harness Plan

### Which Existing Suites to Run

crt-026 touches server tool logic (`context_store`, `context_search`) and store/retrieval
behavior. Per the suite selection table:

| Reason | Suites |
|--------|--------|
| Any server tool logic | `tools`, `protocol` |
| Store/retrieval behavior | `tools`, `lifecycle`, `edge_cases` |
| Any change at all (minimum gate) | `smoke` |

**Mandatory minimum gate**: `pytest -m smoke` must pass before Gate 3c.

**Recommended full run**: `tools`, `lifecycle`, `edge_cases`, `protocol`.

The `confidence`, `contradiction`, `security`, and `volume` suites are not directly relevant
to crt-026 (no confidence formula changes, no contradiction detection changes, no security
boundary changes, no schema changes). Run as regression baseline only (full suite pre-merge).

### New Integration Tests Needed

crt-026 introduces MCP-visible behavior that the existing suites do not exercise:
session-aware search ranking (histogram boost visible across `context_store` → `context_search`
multi-step flows).

The `lifecycle` suite is the correct home for these tests. Add to `suites/test_lifecycle.py`:

**Test 1: `test_session_histogram_boosts_category_match`**
- Fixture: `server` (fresh DB, function scope)
- Setup: register a session; store 3 entries with `category="decision"`, 0 other categories
- Action: `context_search` with the same `session_id` and a generic query
- Assert: results in category `"decision"` rank higher (or equal — if only one category present,
  all matching entries receive same boost; assert no error, no score = NaN)
- Rationale: validates the end-to-end store→histogram→search pipeline through MCP

**Test 2: `test_cold_start_session_search_no_regression`**
- Fixture: `populated_server` (50 pre-loaded entries)
- Setup: no stores in the session before searching
- Action: `context_search` with `session_id` set, then again without `session_id`
- Assert: result order is identical; no error; no NaN scores in either call
- Rationale: AC-08 via the MCP interface (cold-start parity)

**Test 3: `test_duplicate_store_histogram_no_inflation`**
- Fixture: `server`
- Setup: register session; store same entry twice (same content → duplicate detection)
- Action: `context_search` in that session
- Assert: no crash; tool returns normal response (histogram internally stays at count=1)
- Rationale: R-03 through MCP interface

These three tests belong in `suites/test_lifecycle.py` using the `server` / `populated_server`
fixture. They do NOT require harness infrastructure changes — only new test functions.

### When NOT to Add Integration Tests

- `phase_histogram_norm` arithmetic: fully covered by unit tests (`fused-score.md`).
- `format_compaction_payload` histogram block: covered by unit test in `uds.md`.
- `FusionWeights::effective()` NLI-absent behavior: unit-level pure function test.
- `InferenceConfig` validation: unit-level struct test.

---

## Test Execution Plan (Stage 3c)

```bash
# Step 1: Unit tests
cargo test --workspace 2>&1 | tail -30

# Step 2: Smoke gate (mandatory)
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60

# Step 3: Relevant suites
python -m pytest suites/test_tools.py suites/test_lifecycle.py suites/test_edge_cases.py suites/test_protocol.py -v --timeout=60
```

---

## Knowledge Stewardship

- Queried: `context_search` (category: decision, topic: crt-026) — found ADRs #3161–#3175.
- Queried: `session scoring integration test patterns edge cases` — found #707 (behavior-based
  status penalty tests, crt-013), confirming test structure for R-08 penalty ordering.
- Nothing novel to store yet — patterns will be assessed after Stage 3c execution.
