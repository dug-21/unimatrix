# Test Plan Overview: crt-025 — WA-1: Phase Signal + FEATURE_ENTRIES Tagging

GH #330 | Schema v14 → v15

---

## Overall Test Strategy

crt-025 spans three crates (`unimatrix-store`, `unimatrix-server`, `unimatrix-observe`) and
introduces a causal timing guarantee (R-01: synchronous mutation before DB spawn) that cannot
be inferred from static analysis alone. The strategy is layered:

1. **Unit tests** — Pure functions and state machines: validation logic, phase string normalization,
   `SessionState` mutation rules, `build_phase_narrative` pure function, `CategoryAllowlist` count.
2. **Store-level integration tests** — Schema migration (v14→v15 idempotency pattern from
   `migration_v13_to_v14.rs`), `insert_cycle_event`, `record_feature_entries` with phase column,
   both write paths (direct pool and analytics drain).
3. **Server-level integration tests** — Causal phase-tagging chain: cycle event → SessionState
   mutation → context_store phase capture. Phase snapshot skew (R-02) cannot be unit-tested;
   it requires the analytics drain queue to be exercised end-to-end.
4. **infra-001 integration tests** — MCP-protocol-level verification of `context_cycle` new
   parameters, `context_cycle_review` phase narrative, `context_store` outcome-category rejection.

The two CRITICAL risks (R-01, R-02) each require a dedicated causal integration test that cannot
be satisfied by unit tests alone; see §Risk-to-Test Mapping below.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Test Location | Test Function(s) |
|---------|----------|--------------|-----------------|
| R-01 | Critical | Unit (session.rs) + Store integration | `test_set_current_phase_sync_before_store`, `test_phase_end_then_store_sees_new_phase`, `test_stop_then_store_sees_null_phase`, `test_start_with_next_phase_then_store` |
| R-02 | Critical | Store integration (analytics drain) | `test_analytics_drain_uses_enqueue_time_phase`, `test_feature_entry_phase_not_overwritten_by_drain_time_state` |
| R-03 | High | Unit (categories.rs) + infra-001 tools suite | `test_category_allowlist_has_seven_categories`, `test_outcome_category_rejected`, `test_remaining_seven_categories_valid`, `test_cycle_outcome_category_rejected` |
| R-04 | High | Unit + store integration (phase_narrative.rs) | `test_cross_cycle_comparison_zero_prior`, `test_cross_cycle_comparison_one_prior`, `test_cross_cycle_comparison_two_prior`, `test_cross_cycle_excludes_current_feature` |
| R-05 | High | Store integration (migration_v14_to_v15.rs) | `test_v14_to_v15_migration_adds_cycle_events_table`, `test_v14_to_v15_migration_adds_phase_column`, `test_v14_to_v15_migration_idempotent`, `test_schema_version_is_15_after_migration` |
| R-06 | High | Unit (validation.rs) | `test_validate_phase_lowercase_normalization`, `test_validate_phase_uppercase_normalization`, `test_validate_phase_space_rejected`, `test_validate_phase_empty_rejected`, `test_validate_phase_64_char_boundary`, `test_validate_phase_65_char_rejected`, `test_validate_next_phase_normalization` |
| R-07 | Medium | Unit + store integration | `test_seq_monotonic_three_sequential_events`, `test_cycle_events_ordering_by_timestamp_seq` |
| R-08 | High | Unit (phase_narrative.rs) + infra-001 lifecycle suite | `test_phase_narrative_absent_when_no_cycle_events`, `test_phase_narrative_present_when_events_exist`, `test_retrospective_report_no_null_phase_narrative_key` |
| R-09 | Medium | Unit (hook.rs) | `test_hook_phase_end_invalid_phase_logs_warning_no_error`, `test_hook_phase_end_empty_phase_falls_through`, `test_hook_phase_end_valid_emits_cycle_phase_end` |
| R-10 | High | Store integration (migration_v14_to_v15.rs) | `test_fresh_db_creates_schema_v15`, `test_fresh_db_has_cycle_events_table`, `test_fresh_db_has_feature_entries_phase_column` |
| R-11 | High | Compile check + store integration | `test_analytics_drain_phase_some_persisted`, `test_analytics_drain_phase_none_persisted_as_null` |
| R-12 | Medium | Unit (phase_narrative.rs) | `test_cross_cycle_mean_excludes_current_feature_data` |
| R-13 | Medium | Unit (phase_narrative.rs) | `test_build_phase_narrative_orphaned_phase_end_no_start`, `test_build_phase_narrative_phase_end_only_no_panic` |
| R-14 | High | Compile check + server integration | `test_context_store_active_phase_writes_non_null`, `test_context_store_no_phase_writes_null`, `test_usage_context_current_phase_propagates` |

---

## Cross-Component Test Dependencies

The critical causal chain spans three components:

```
UDS Listener (Component 5)
  synchronous set_current_phase()
      |
      v
SessionState (Component 4)
  current_phase = Some("scope")
      |
      v (context_store reads snapshot)
Context Store Phase Capture (Component 8)
  snapshot = Some("scope")
      |
      v
Store Layer (Component 6)
  feature_entries.phase = "scope"
```

Tests for R-01 must instrument or observe all four components. The store-level integration
test is the only feasible approach: it must call the full `context_store` path (not just
`record_feature_entries`) to verify the snapshot is taken at the right moment.

The analytics drain chain (R-02) has a separate path:
```
AnalyticsWrite::FeatureEntry { phase: Some("implementation") }  ← enqueue
  ...time passes...
SessionState.current_phase → Some("testing")  ← phase advanced
  ...drain fires...
feature_entries.phase = "implementation"  ← must be enqueue-time value
```

This requires a test that enqueues the drain event, advances SessionState, then flushes the
drain and asserts the persisted phase value matches the enqueue-time snapshot.

---

## Integration Harness Plan (infra-001)

### Suites to Run

This feature touches server tool logic (`context_cycle`, `context_store`, `context_cycle_review`),
store/retrieval behavior, schema changes, and category allowlist changes. Per the suite selection
table:

| Suite | Reason to Run |
|-------|--------------|
| `smoke` | Mandatory minimum gate — every change |
| `tools` | `context_cycle` new params, `context_store` outcome-category rejection, `context_cycle_review` new response fields |
| `lifecycle` | Phase-tag chain: store→cycle→review multi-step flow; restart persistence of `CYCLE_EVENTS` |
| `edge_cases` | Phase string edge cases (Unicode phase tokens, boundary lengths) visible at MCP level |
| `adaptation` | `CategoryAllowlist` change: outcome retirement affects `test_category_allowlist_*` tests in this suite |

Suites NOT required: `confidence` (no confidence formula changes), `contradiction` (no NLI changes),
`security` (no new security boundaries), `volume` (no query performance changes; new queries have
`cycle_events(cycle_id)` index coverage), `protocol` (MCP handshake unchanged).

### New Integration Tests Required in infra-001

The following MCP-visible behaviors have no existing suite coverage and require new tests:

#### 1. `suites/test_tools.py` — New tests for `context_cycle` and `context_cycle_review`

**`test_cycle_phase_end_type_accepted`** (`server` fixture)
- Call `context_cycle(type="phase-end", topic="crt-025-test", phase="scope", next_phase="design")`
- Assert success response

**`test_cycle_phase_end_stores_row`** (`server` fixture)
- Call `context_cycle(type="start", topic="crt-025-test", next_phase="scope")`
- Call `context_cycle(type="phase-end", topic="crt-025-test", phase="scope", next_phase="design")`
- Call `context_cycle(type="stop", topic="crt-025-test")`
- Assert success for all three; implicitly verifies three CYCLE_EVENTS rows via cycle_review

**`test_cycle_invalid_type_rejected`** (`server` fixture)
- Call `context_cycle(type="pause", topic="crt-025-test")`
- Assert error response containing valid type names

**`test_cycle_phase_with_space_rejected`** (`server` fixture)
- Call `context_cycle(type="phase-end", topic="crt-025-test", phase="scope review")`
- Assert error response mentioning `phase` field

**`test_cycle_outcome_category_rejected`** (`server` fixture)
- Call `context_store(content="...", topic="testing", category="outcome", agent_id="human")`
- Assert error response (InvalidCategory)

**`test_cycle_review_includes_phase_narrative`** (`server` fixture)
- Seed: `start` + `phase-end` + `stop` for a topic
- Call `context_cycle_review(feature_cycle="<topic>", format="json")`
- Assert JSON response contains `phase_narrative` key with non-null value

**`test_cycle_review_no_phase_narrative_for_old_feature`** (`server` fixture)
- Call `context_cycle_review(feature_cycle="nonexistent-old-feature", format="json")`
- Assert JSON response does NOT contain `phase_narrative` key (or `phase_narrative` is absent)

#### 2. `suites/test_lifecycle.py` — New test for phase-tag lifecycle flow

**`test_phase_tag_store_cycle_review_flow`** (`server` fixture)
- `context_cycle(type="start", topic="crt-025-test", next_phase="scope")`
- `context_store(content="...", topic="crt-025-test", category="decision", agent_id="human")`
- `context_cycle(type="phase-end", topic="crt-025-test", phase="scope", next_phase="design")`
- `context_store(content="...", topic="crt-025-test", category="decision", agent_id="human")`
- `context_cycle(type="stop", topic="crt-025-test")`
- `context_cycle_review(feature_cycle="crt-025-test", format="json")`
- Assert `phase_narrative` is present; `phase_sequence` is non-empty; `rework_phases` is a list

#### 3. `suites/test_adaptation.py` — Update existing `outcome` category tests

The adaptation suite may contain tests using `category="outcome"`. Any such test must be updated
to expect an error after crt-025 retirement, or the category changed to a valid one.

### Tests NOT needed in infra-001

- `seq` monotonicity under concurrent sessions: internal storage detail, not MCP-visible.
- Analytics drain phase snapshot: requires internal state manipulation not possible through MCP.
- `SessionState.set_current_phase` internals: unit tests suffice.
- `build_phase_narrative` pure function: unit tests in `unimatrix-observe` suffice.

---

## Test File Structure

```
crates/unimatrix-store/tests/
  migration_v14_to_v15.rs         (new — R-05, R-10, AC-10, AC-11)

crates/unimatrix-server/src/
  infra/validation.rs              (inline tests — R-06, AC-02, AC-03)
  infra/session.rs                 (inline tests — R-01, AC-06)
  infra/categories.rs              (inline tests — R-03, AC-15)
  uds/hook.rs                      (inline tests — R-09, AC-16)
  mcp/tools.rs                     (inline tests — AC-01, AC-05, AC-07, AC-08)

crates/unimatrix-observe/src/
  phase_narrative.rs               (inline tests — R-04, R-08, R-12, R-13, AC-12)

product/test/infra-001/suites/
  test_tools.py                    (extend — R-03, R-06, R-08, AC-02, AC-15)
  test_lifecycle.py                (extend — R-01, AC-05, AC-07, AC-08, AC-12)
  test_adaptation.py               (update — outcome category tests)
```

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: "procedure") for testing procedures — found #229
  (Tester duties), #165 (delivery session flow), #729 (intelligence pipeline testing pattern),
  #129 (concrete assertions convention). The cross-crate integration pattern (#729) confirms
  that the analytics drain path (R-02) must be tested at the store-crate level, not server level.
- Stored: nothing novel to store at plan stage — patterns already match existing conventions.
  The phase-snapshot-at-enqueue test pattern (R-02) is a direct application of #2125 already in
  Unimatrix. Will revisit after Stage 3c execution.
