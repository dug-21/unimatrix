# col-023 Test Plan Overview

## Test Strategy

This feature touches four crates (`unimatrix-core`, `unimatrix-observe`, `unimatrix-store`,
`unimatrix-server`) via a four-wave compilation-gated refactor. The testing strategy has
three tiers:

| Tier | Scope | Tool |
|------|-------|------|
| Unit | Per-component logic, domain guard correctness, DSL evaluation, security bounds, schema round-trips | `cargo test -p <crate>` |
| Integration (workspace) | Cross-crate boundary correctness, detection pipeline with real ObservationRecord slices | `cargo test --workspace` |
| Integration (infra-001) | MCP-level behavior: schema persistence, lifecycle correctness, tool shape invariance | `python -m pytest` |

### Test Execution Order

1. After Wave 1 (unimatrix-core): `cargo check --workspace`, then unit tests for `observation-record` component.
2. After Wave 2 (unimatrix-observe `domain/` + `metrics.rs`): `cargo check`, then unit tests for `domain-pack-registry`, `rule-dsl-evaluator`, `metrics-extension`.
3. After Wave 3 (detection rules rewrite): `cargo check`, then unit tests for `detection-rules`.
4. After Wave 4 (unimatrix-server + test fixture updates): `cargo test --workspace`, then all infra-001 suites.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Test Location | Test Names |
|---------|----------|---------------|------------|
| R-01 | Critical | `unimatrix-observe/tests/detection_isolation.rs` (new) | `test_*_no_findings_for_unknown_domain`, `test_mixed_domain_slice_*` |
| R-02 | Critical | `unimatrix-observe/tests/detection_isolation.rs` (new) | `test_backward_compat_snapshot_*`, per-rule regression fixtures |
| R-03 | Critical | Static grep + `cargo test -p unimatrix-observe` count | `grep -r 'source_domain: ""'` post-Wave-4 |
| R-05 | High | `unimatrix-store/src/migration.rs` tests | `test_v13_to_v14_migration_round_trip`, `test_v13_row_reads_null_domain_metrics` |
| R-06 | High | `unimatrix-observe` unit tests + infra-001 | `test_parse_rows_unknown_event_type_passthrough`, lifecycle suite |
| R-07 | High | `unimatrix-observe` unit tests | `test_temporal_window_unsorted_input_fires`, `test_temporal_window_sorted_vs_unsorted_equivalent` |
| R-08 | High | `unimatrix-observe` unit tests | `test_threshold_field_path_non_numeric_no_panic`, `test_threshold_field_path_missing_key` |
| R-09 | High | `unimatrix-server` unit + infra-001 | `test_registry_startup_missing_rule_file`, `test_registry_startup_malformed_rule` |
| R-10 | Medium | `unimatrix-observe` unit tests | `test_registry_duplicate_category_idempotent`, `test_registry_invalid_category_rejected` |
| R-11 | Medium | `unimatrix-store` unit tests | `test_universal_metrics_fields_count_22`, `test_universal_metrics_fields_names_unchanged` |
| R-12 | Medium | `unimatrix-store/src/migration.rs` tests | `test_v14_schema_named_column_readback` |
| R-13 | Medium | Static grep post-Wave-3 | `grep -r "HookType::"` zero matches |

---

## Non-Negotiable Tests (Gate 3c)

These must all pass for Gate 3c to proceed:

1. **R-01**: Every one of the 21 rewritten rules has a mixed-domain test — `source_domain = "unknown"` records do not trigger claude-code rules.
2. **R-02**: Backward compatibility snapshot test — identical `RetrospectiveReport` for fixed claude-code fixture (AC-04).
3. **R-04** (structural): `DomainPackRegistry` has no MCP write path — code review + unit test that only `load_from_config()` is a write method (AC-08).
4. **R-06**: `parse_observation_rows` unknown event passthrough test (AC-11).
5. **R-07**: Temporal window rule fires on unsorted input (or produces equivalent result).
6. **R-03**: Static verification — zero `source_domain: ""` in test fixtures (grep-based).
7. **AC-02**: `cargo test -p unimatrix-observe` test count does not decrease from pre-feature baseline.

---

## Cross-Component Test Dependencies

| Upstream Component | Downstream Test | Why |
|-------------------|-----------------|-----|
| `observation-record` (unimatrix-core) | ALL detection-rules, metrics-extension, domain-pack-registry tests | Every test constructs `ObservationRecord` |
| `domain-pack-registry` | `detection-rules` integration tests | Mixed-domain tests require a real `DomainPackRegistry` |
| `schema-migration` (v14) | `metrics-extension` storage tests | `store_metrics()` / `get_metrics()` tests require v14 schema |
| `ingest-security` bounds | `detection-rules` + `domain-pack-registry` tests | Security bounds are applied before records reach detection |

---

## Integration Harness Plan

### Which Existing Suites Apply

This feature makes no changes to MCP tool signatures, knowledge store logic, or the
`context_cycle_review` response schema. The behavioral change is internal (domain
dispatch, metric schema). The applicable suite selection from USAGE-PROTOCOL.md is:

| What col-023 Touches | Applicable Suite |
|----------------------|-----------------|
| Schema change (`OBSERVATION_METRICS` v13→v14) | `lifecycle` (restart persistence), `volume` (schema at scale) |
| Security bounds at ingest | `security` |
| Any change at all | `smoke` (mandatory minimum gate) |
| No MCP tool logic changes | `tools` suite NOT required |
| No confidence system changes | `confidence` suite NOT required |

**Suites to run at Stage 3c (in order):**
1. `smoke` — mandatory gate
2. `lifecycle` — restart persistence verifies schema migration survived restart
3. `security` — existing security suite as regression baseline (no new security surface to MCP layer, but existing security boundary must not regress)

### Gap Analysis — New Integration Tests NOT Needed via infra-001

The following col-023 behaviors are NOT visible through the MCP JSON-RPC interface and
therefore do NOT require new infra-001 tests:

- `DomainPackRegistry` initialization and `source_domain` resolution — purely internal server startup behavior, no MCP tool exposes registry state.
- `parse_observation_rows` security bounds — observations are ingested via UDS (hook path), not MCP. The existing `security` suite covers the MCP input validation surface only.
- Detection rule `source_domain` isolation — `context_cycle_review` output shape is unchanged; finding counts are not directly inspectable via MCP in a way that allows domain isolation assertions.
- Schema v13→v14 migration — the `OBSERVATION_METRICS` table is not directly queryable via MCP. Lifecycle tests verifying restart persistence are sufficient.

**Conclusion**: No new infra-001 tests are required for col-023. The new behavior is
tested entirely at the unit and workspace integration level. The existing `lifecycle`
suite already validates restart-persistent storage including `OBSERVATION_METRICS`
round-trips. If the `lifecycle` suite passes post-migration, the schema change is
validated end-to-end.

### If integration test failures are encountered at Stage 3c

Apply the triage protocol from USAGE-PROTOCOL.md:
- Failure in code this feature changed → fix, re-run, document.
- Pre-existing failure → file GH Issue with `[infra-001]` prefix, add `@pytest.mark.xfail(reason="Pre-existing: GH#NNN")`, continue.
- Bad test assertion → fix test, document in report.

---

## Test File Structure

New test files to create:

```
crates/unimatrix-observe/tests/detection_isolation.rs   -- R-01, R-02 (21-rule mixed-domain + snapshot)
crates/unimatrix-observe/tests/domain_pack_tests.rs     -- domain-pack-registry unit tests
```

Existing files to update:

```
crates/unimatrix-observe/tests/extraction_pipeline.rs   -- update ObservationRecord construction (R-03)
crates/unimatrix-store/src/migration.rs                 -- add v13→v14 tests inline (R-05, R-11, R-12)
```

All new tests in `unimatrix-observe/tests/` must supply both `event_type` and a
non-empty `source_domain` on every `ObservationRecord` construction site (R-03 obligation).
