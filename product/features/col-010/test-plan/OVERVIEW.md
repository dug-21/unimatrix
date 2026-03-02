# Test Plan Overview: col-010

Feature: Session Lifecycle Persistence & Structured Retrospective
Phase: Stage 3a — Test Plan Design
Date: 2026-03-02

---

## Overall Test Strategy

col-010 adds two new redb tables (SESSIONS, INJECTION_LOG), four new store operations, one new observe module, and modifies three server-side tool handlers. Testing proceeds in P0-first order matching the implementation priority.

### Test Tiers

| Tier | Scope | Framework |
|------|-------|-----------|
| Unit | Store operations, serialization, GC logic, narrative synthesis, score computation | `cargo test` (Rust) |
| Integration | Hook event → persistent record roundtrip; retrospective path selection; schema migration | `cargo test` with tmpdir store |
| MCP Integration | evidence_limit parameter; tool response structure; provenance boost in search ranking | infra-001 pytest harness |

---

## Risk-to-Test Mapping

| Risk ID | Risk Description | Priority | Test Component | Coverage Strategy |
|---------|-----------------|----------|----------------|-------------------|
| R-01 | `next_log_id` counter race during restart mid-migration | Critical | storage-layer | Idempotency test: call migrate_v4_to_v5 twice on same open txn; verify counter not reset |
| R-02 | GC cascade atomicity — SESSIONS deleted, INJECTION_LOG orphans survive | High | session-gc | Atomic rollback test: inject fault after INJECTION_LOG delete; verify SESSIONS rollback |
| R-03 | `total_injections` accuracy — fire-and-forget discrepancy | High | uds-listener | Document as known limitation; test that in-memory count matches at least when tasks flush |
| R-04 | Abandoned session filter missing in retrospective | High | structured-retrospective | Include Abandoned sessions; verify session_count excludes them |
| R-05 | Batch INJECTION_LOG write latency spike | Medium | uds-listener | Test that one ContextSearch → one transaction (not N) |
| R-06 | Fire-and-forget ONNX failure for lesson-learned | Medium | lesson-learned | Mock ONNX failure; verify entry still written with embedding_dim=0 |
| R-07 | Provenance boost applied at two callsites | Medium | lesson-learned | Integration test verifying both MCP and hook search paths apply boost |
| R-08 | Structured retrospective crate boundary violation | Medium | structured-retrospective | Verify unimatrix-observe has no dependency on unimatrix-store |
| R-09 | evidence_limit default changes existing caller behavior | High | tiered-output | Audit + update all integration tests asserting on evidence array lengths |
| R-10 | Auto-outcome writes with unsanitized feature_cycle / agent_role | Medium | auto-outcomes | Test sanitization strips control chars and truncates at 128 chars |

---

## Cross-Component Test Dependencies

- `storage-layer` tests must pass before `uds-listener` tests can run (inject_session needs working store).
- `auto-outcomes` tests depend on `outcome_tags.rs` update (VALID_TYPES includes "session").
- `structured-retrospective` tests depend on `storage-layer` (needs scan_sessions_by_feature, scan_injection_log_by_session).
- `tiered-output` integration tests (R-09) must be audited BEFORE implementing P1 Component 6.
- `lesson-learned` provenance boost tests must verify both search paths.

---

## Integration Harness Plan

### Applicable Suites

| Suite | Reason |
|-------|--------|
| `smoke` | Mandatory gate — minimum regression check |
| `tools` | New `evidence_limit` parameter on `context_retrospective`; `context_status` with maintain=true triggers GC |
| `lifecycle` | Session persistence roundtrip: SessionRegister → ContextSearch → SessionClose → restart → read |
| `security` | `session_id` sanitization; `agent_role`/`feature_cycle` sanitization |

### New Integration Tests to Add (Stage 3c)

Located in `product/test/infra-001/suites/`:

#### test_tools.py — evidence_limit tests

| Test Name | Validates |
|-----------|----------|
| `test_retrospective_evidence_limit_default` | Default (no parameter) → each hotspot ≤ 3 evidence items |
| `test_retrospective_evidence_limit_zero` | `evidence_limit=0` → full arrays returned (backward compat) |
| `test_retrospective_evidence_limit_custom` | `evidence_limit=5` → each hotspot ≤ 5 items |

#### test_lifecycle.py — session persistence tests

| Test Name | Validates |
|-----------|----------|
| `test_session_register_persists` | SessionRegister event → session readable after server restart |
| `test_session_close_updates_status` | SessionClose → status=Completed or Abandoned in persisted record |
| `test_injection_log_written_per_search` | ContextSearch with N results → N InjectionLogRecords |

#### test_security.py — sanitization tests

| Test Name | Validates |
|-----------|----------|
| `test_session_register_invalid_id_rejected` | session_id with `!` → error response |
| `test_session_register_long_id_rejected` | session_id > 128 chars → error response |

### R-09 Test Audit Protocol (BLOCKING for P1)

Before implementing tiered-output (Component 6):

1. Run: `grep -rn "evidence" product/test/infra-001/suites/`
2. For each assertion on `hotspots[N]["evidence"]` array length:
   - Add `evidence_limit=0` to the request parameters in that test, OR
   - Update the assertion to `<= 3` if the test is deliberately checking limited output.
3. Document the audit results in the agent report.

### Fixtures to Use

| Fixture | Used In |
|---------|---------|
| `server` | Most tool tests (fresh DB per test) |
| `lifecycle` | Session persistence roundtrip (state must accumulate) |

---

## Acceptance Criteria Verification Summary

| AC-ID | Priority | Test File | Test Type |
|-------|----------|-----------|-----------|
| AC-01 | P0 | storage-layer.md | Rust integration |
| AC-02 | P0 | uds-listener.md | Rust integration |
| AC-03 | P0 | uds-listener.md | Rust integration |
| AC-04 | P0 | uds-listener.md | Rust integration |
| AC-05 | P0 | uds-listener.md | Rust integration |
| AC-06 | P0 | uds-listener.md | Rust integration |
| AC-07 | P0 | storage-layer.md | Rust unit |
| AC-08 | P0 | session-gc.md | Rust integration |
| AC-09 | P0 | session-gc.md | Rust integration |
| AC-10 | P0 | auto-outcomes.md | Rust unit |
| AC-11 | P0 | auto-outcomes.md | Rust integration |
| AC-12 | P0 | structured-retrospective.md | Rust integration |
| AC-13 | P0 | structured-retrospective.md | Rust integration |
| AC-14 | P0 | storage-layer.md | Rust integration |
| AC-15 | P1 | tiered-output.md + infra-001 | Rust + MCP |
| AC-16 | P1 | tiered-output.md + infra-001 | Rust + MCP |
| AC-17 | P1 | structured-retrospective.md | Rust integration |
| AC-18 | P1 | structured-retrospective.md | Rust unit |
| AC-19 | P1 | structured-retrospective.md | Rust unit |
| AC-20 | P1 | lesson-learned.md | Rust integration |
| AC-21 | P1 | lesson-learned.md | Rust integration |
| AC-22 | P1 | lesson-learned.md | Rust integration |
| AC-23 | P1 | lesson-learned.md | Rust unit + integration |
| AC-24 | P0+P1 | all | cargo test --workspace |
