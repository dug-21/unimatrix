# Gate 3c Report: col-023

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-21
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 13 active risks have passing tests; 2 closed (R-04, R-14) |
| Test coverage completeness | PASS | All risk-to-scenario mappings exercised; 3,029 unit + 66 integration |
| Specification compliance | PASS | All 11 AC items verified (AC-01 through AC-11, no AC-08 old) |
| Architecture compliance | PASS | Component boundaries, ADRs, no architectural drift |
| Knowledge stewardship (tester) | PASS | Both tester reports have stewardship blocks with Queried + Stored entries |

Non-negotiable checks:

| Non-Negotiable | Required By | Status |
|----------------|-------------|--------|
| 21 per-rule mixed-domain isolation tests in detection_isolation.rs | R-01 | PASS — 21 tests confirmed |
| Backward compat snapshot (T-DET-COMPAT-02) | R-02 | PASS — present in detection_isolation.rs |
| DomainPackRegistry has no MCP write path (AC-08) | R-04 | PASS — static + unit verified |
| Unknown event passthrough test (AC-11) | R-06 | PASS — test_parse_rows_unknown_event_type_passthrough |
| Temporal window unsorted input tested | R-07 | PASS — test_temporal_window_unsorted_input_fires |

---

## Detailed Findings

### Risk Mitigation Proof

**Status**: PASS

**Evidence**:

RISK-COVERAGE-REPORT.md maps all 13 active risks (R-01 through R-13) to passing tests. R-04 (closed) and R-14 (closed) are correctly marked N/A due to FR-06 removal.

Key verifications:

- **R-01** (cross-domain false findings): 21 per-rule tests confirmed present in `detection_isolation.rs` — each constructs records with `source_domain = "sre"` using event_type/tool values that WOULD fire the rule if domain were "claude-code", and asserts zero findings. Static verification also confirms all 21 rules have `.filter(|r| r.source_domain == "claude-code")` as first operation: agent.rs (7 rules, 7 guards), friction.rs (4/4), session.rs (5/5), scope.rs (5/5). Note: RISK-COVERAGE-REPORT listed only 3 guard line numbers for scope.rs but actual code shows 5 guards — minor documentation error, not a coverage gap.

- **R-02** (backward compat): `test_retrospective_report_backward_compat_claude_code_fixture` present at line 646 of `detection_isolation.rs`. Runs full detection + metrics pipeline against hardcoded claude-code fixture (2 agent spawns, 20 Read calls, 8 compile commands, 1 sleep, 3-hour session gap, task completion). Asserts Agent, Friction, and Session categories present in findings, `total_tool_calls > 0`, `computed_at` preserved. Named as T-DET-COMPAT-02 per strategy requirement.

- **R-04** (structural: no MCP write path): `test_domain_pack_registry_no_runtime_write_path` at line 274 of domain_pack_tests.rs. Static grep confirms `grep -rn "DomainPackRegistry" crates/unimatrix-server/src/mcp/` returns zero matches. Public API of `DomainPackRegistry` has only read methods (`lookup`, `rules_for_domain`, `resolve_source_domain`, `iter_packs`) plus constructors (`new`, `with_builtin_claude_code`) — no runtime mutation methods.

- **R-06** (unknown event passthrough): `test_parse_rows_unknown_event_type_passthrough` confirmed at `crates/unimatrix-server/src/services/observation.rs:1116`.

- **R-07** (temporal window unsorted): `test_temporal_window_unsorted_input_fires` and `test_temporal_window_sorted_vs_unsorted_equivalent` confirmed in domain_pack_tests.rs at lines 561 and 579.

---

### Test Coverage Completeness

**Status**: PASS

**Evidence**:

Unit test count: 3,029 passing (3,140 total across all suites in a clean run), 27 ignored (pre-existing unimatrix-embed). All results verified via `cargo test --workspace`.

Per-crate breakdown matches RISK-COVERAGE-REPORT.md:
- unimatrix-observe lib: 357 (PASS)
- detection_isolation (new): 22 (PASS) — 21 isolation + 1 backward compat
- domain_pack_tests (updated): 44 (PASS) — includes 6 rework additions
- unimatrix-server lib: 1,721 (PASS) — includes ingest security tests
- unimatrix-store lib: 136 (PASS) — includes structural tests
- Migration v13→v14 (`--features test-support`): 8 (PASS)

Integration tests (infra-001):
- Smoke: 20 passed — mandatory gate confirmed PASS
- Lifecycle: 27 passed, 1 xfailed (GH#305, pre-existing `test_retrospective_baseline_present`) — PASS
- Security: 19 passed — PASS

The single lifecycle xfail has a corresponding GH issue (#305) and is pre-existing and unrelated to col-023. No integration tests deleted or commented out.

unimatrix-observe baseline comparison (AC-02): pre-feature 359, post-rework 429 (+70). Non-decreasing: PASS.

All risk-to-scenario mappings from Phase 2 are exercised:
- R-01: 21 per-rule isolation + DSL + metrics tests
- R-02: backward compat fixture smoke
- R-03: static grep (zero matches) + test count non-regression
- R-05: full migration round-trip (8 tests)
- R-06: passthrough unit tests
- R-07: unsorted-input temporal window tests
- R-08: field_path non-numeric/missing/empty tests
- R-09: startup failure tests (3 paths)
- R-10: duplicate/invalid category tests
- R-11: structural count and per-field alignment tests
- R-12: named-column readback with reduced struct
- R-13: static grep (zero HookType:: matches)

Integration risks IR-01 through IR-04 are covered by the combined AC-03, AC-05, and AC-04 tests.

---

### Specification Compliance

**Status**: PASS

**Evidence**:

All 11 acceptance criteria verified:

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `grep -r "hook: HookType" crates/` returns only comments/docstrings in observation.rs; `ObservationRecord` has `event_type: String` + `source_domain: String` confirmed in unimatrix-core/src/observation.rs:24-28 |
| AC-02 | PASS | unimatrix-observe test count 429 >= 359 baseline |
| AC-03 | PASS | `test_default_config_loads_claude_code_pack` and `test_with_builtin_claude_code_pack_always_loads` in domain_pack_tests.rs |
| AC-04 | PASS | `test_retrospective_report_backward_compat_claude_code_fixture` in detection_isolation.rs |
| AC-05 | PASS | `test_threshold_source_domain_guard_isolation` + 21 per-rule isolation tests confirm claude-code rules do not fire on sre/unknown domain records |
| AC-06 | PASS | Payload size and nesting depth boundary tests in services::observation::tests |
| AC-07 | PASS | `test_source_domain_invalid_cases_all_reject` and `test_registry_rejects_invalid_source_domain_formats` |
| AC-08 | PASS | Unit test + static grep; DomainPackRegistry exposes no MCP write path |
| AC-09 | PASS | 8-test migration v13→v14 suite: schema v14, column presence, round-trip, NULL readback |
| AC-10 | PASS | `UNIVERSAL_METRICS_FIELDS.len() == 22` structural test; 22nd entry is `"domain_metrics_json"`; first 21 entries verified by name against SQL column declaration order |
| AC-11 | PASS | `test_parse_rows_unknown_event_type_passthrough` in observation.rs |

Functional requirements FR-01 through FR-05 are all satisfied.
- FR-01: `ObservationRecord` has `event_type` + `source_domain` replacing `hook: HookType`; `HookType` preserved as `pub mod hook_type` constants in unimatrix-core.
- FR-02: `DomainPackRegistry` (`Arc<RwLock<HashMap<String, DomainPack>>>`) with startup initialization from TOML; `[serde(default)]` absent-section handling.
- FR-03: `parse_observation_rows` no longer drops unknown event types; security bounds (64 KB, depth 10, domain regex) applied at ingest.
- FR-04: All 21 rules rewritten with string-based matching; `source_domain` guard as first filter; DSL evaluator supports threshold + temporal window.
- FR-05: `UNIVERSAL_METRICS_FIELDS` has 22 entries; `domain_metrics_json TEXT NULL` added to OBSERVATION_METRICS in v14 migration; schema version = 14.

Non-functional requirements:
- NFR-01 (backward compat): verified by AC-04 tests.
- NFR-02 (payload limits): verified by AC-06.
- NFR-03 (domain validation): verified by AC-07.
- NFR-04 (rule sandboxing): enforced by DSL evaluator design (threshold/temporal window only, no eval).
- NFR-05 (no new dependencies): no new crate dependencies added (verified by cargo.lock review in gate-3b).
- NFR-06 (no wire protocol changes): observation.rs wire types unchanged.
- NFR-07 (no observations table migration): confirmed — only OBSERVATION_METRICS changes.
- NFR-08 (schema v14): `CURRENT_SCHEMA_VERSION = 14` confirmed in migration.rs:19.
- NFR-09 (compilation gates): all four waves compiled cleanly (verified in gate-3b).

---

### Architecture Compliance

**Status**: PASS

**Evidence**:

Component boundaries match approved architecture:
- `unimatrix-core/src/observation.rs`: `ObservationRecord` struct with `event_type` + `source_domain` — matches ARCHITECTURE.md spec.
- `unimatrix-observe/src/domain/`: `DomainPack`, `DomainPackRegistry`, `RuleDescriptor`, `RuleEvaluator` — new module confirmed present.
- `unimatrix-observe/src/detection/`: 4 modules (agent/friction/session/scope) with string-based matching + source_domain guard.
- `unimatrix-store/src/metrics.rs`: `UNIVERSAL_METRICS_FIELDS` 22 entries, `MetricVector` with `domain_metrics` field.
- `unimatrix-server/src/infra/config.rs`: `ObservationConfig` + `DomainPackConfig` structs with `#[serde(default)]`.
- `unimatrix-server/src/services/observation.rs`: `DomainPackRegistry` injected via `Arc` into `SqlObservationSource`.

ADR compliance:
- ADR-001: HookType replaced with string fields — PASS
- ADR-002: Config-only registration, no MCP write path — PASS (zero refs in src/mcp/)
- ADR-003: Two-kind bounded DSL (threshold + temporal_window) — PASS (RuleEvaluator in domain/evaluator.rs)
- ADR-004: Four-wave compilation-gated refactor — PASS (verified in gate-3b)
- ADR-005: Mandatory source_domain guard in all domain-specific rules — PASS (21/21 guards confirmed)
- ADR-006: UNIVERSAL_METRICS_FIELDS as canonical + extension column — PASS
- ADR-007: Payload bounds at parse_observation_rows — PASS

Schema v14 migration: single `ALTER TABLE observation_metrics ADD COLUMN domain_metrics_json TEXT NULL` with idempotency check (`IF NOT EXISTS` via pragma_table_info). Matches ARCHITECTURE.md migration spec.

No architectural drift detected.

---

### Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

- `col-023-agent-10-tester-report.md`: Contains `## Knowledge Stewardship` block with `Queried:` entry (`/uni-knowledge-search` for procedure category) and `Stored: nothing novel to store — migration test pattern follows established pattern`.

- `col-023-agent-10b-test-rework-report.md`: Contains `## Knowledge Stewardship` block with `Queried:` entry (attempted uni-knowledge-search, server unavailable, proceeded without) and `Stored: nothing novel to store — per-rule isolation test pattern is feature-specific`.

Both reports satisfy the stewardship obligation. The "server unavailable" note in the rework report is a valid acknowledgment of a query attempt.

---

## Rework Required

None.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the per-rule isolation test pattern (one test per built-in rule, adversarial domain input, zero-finding assertion) is specific to this feature's cross-domain isolation architecture. If this pattern recurs across 2+ features with domain isolation requirements, it warrants a `pattern` entry in Unimatrix.
