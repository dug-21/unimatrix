# Gate 3b Report: col-023

> Gate: 3b (Code Review)
> Date: 2026-03-21
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | FAIL | `SqlObservationSource` never receives the startup-built `DomainPackRegistry` Arc; all call sites use `new_default()`. Pseudocode step 4 of lib.rs wiring explicitly requires threading the registry into the source. |
| Architecture compliance | PASS | ADR decisions followed; component boundaries maintained; four-wave refactor completed |
| Interface implementation | PASS | Function signatures match pseudocode; data types correct; error handling follows project patterns |
| Test case alignment | PASS | All 21 detection rules tested; security bounds covered; AC-06, AC-07, AC-10, AC-11 tests present |
| Code quality — no stubs | PASS | No `todo!()`, `unimplemented!()`, `FIXME` in col-023 code; TODO items are pre-existing W2-4 markers |
| Code quality — no `.unwrap()` in production | PASS | `.unwrap()` calls in observe crate are all inside `#[cfg(test)]` |
| Code quality — compilation | PASS | `cargo check --workspace` completes clean (zero errors, 9 pre-existing warnings) |
| Code quality — file size | WARN | 7 files exceed 500 lines; all were already over-limit before col-023 |
| Code quality — clippy | WARN | 18 clippy errors in `unimatrix-store` (pre-existing, not introduced by col-023); migration.rs line offset shifted but the lint is in v5→v6 legacy code untouched by this feature |
| Security — no hardcoded secrets | PASS | No secrets or API keys in col-023 code |
| Security — input validation | PASS | Payload size (64 KB) and JSON depth (10 levels) enforced at ingest; `source_domain` regex-validated at config load |
| Security — path traversal | PASS | No file path operations in new col-023 code paths |
| Security — no MCP write path to registry | PASS | `DomainPackRegistry` has no public write method; `load_from_config()` equivalent (`new()`) is called at startup only; no MCP tool handler references a registry mutation method |
| Security — DSL sandboxing | PASS | `RuleEvaluator` is pure data transformation; no filesystem access, no eval, no dynamic loading |
| Security — cargo audit | WARN | `cargo-audit` not installed; cannot verify CVE status |
| Knowledge stewardship | PASS | Agent-9 report has `Queried:` and `Stored:` entries; deviation intentionally documented |
| source_domain guard as first filter (ADR-005) | PASS | All 21 rules apply `source_domain == "claude-code"` guard as their first operation; `compute_universal()` guards on `source_domain == "claude-code"` before any metric computation |
| DomainPackRegistry startup wiring | FAIL | Registry is built and validated at startup but stored in `_observation_registry` (suppressed unused-variable warning); never passed to the 20+ `SqlObservationSource::new_default()` call sites in tools.rs, status.rs, and mcp/tools.rs |
| "unknown" reserved at registration | PASS | `DomainPackRegistry::new()` rejects `source_domain = "unknown"` with `InvalidSourceDomain` error (EC-04) |
| No MCP write path to registry (AC-08) | PASS | No MCP tool handler calls any registry mutation method; confirmed by grep |
| compute_universal() source_domain guard (IR-03) | PASS | `compute_universal()` filters `records.iter().filter(\|r\| r.source_domain == "claude-code")` as its first operation |

---

## Detailed Findings

### 1. Pseudocode Fidelity — DomainPackRegistry Not Threaded Into Request Handlers

**Status**: FAIL

**Evidence**: The pseudocode (`ingest-security.md`, "lib.rs startup wiring", step 4) explicitly requires:
```
-- 4. Thread registry as Arc into SqlObservationSource
let registry_arc = Arc::new(registry)
let obs_source = SqlObservationSource::new(store.clone(), registry_arc)
```

The implementation at `crates/unimatrix-server/src/main.rs:550` builds the registry:
```rust
let _observation_registry = {
    // ... builds Arc<DomainPackRegistry> ...
    Arc::new(reg)
};
```

The `_` prefix suppresses the unused-variable warning. The 20+ call sites where `SqlObservationSource` is instantiated (in `mcp/tools.rs:1115`, `mcp/tools.rs:1368`, `services/status.rs:715`, and 17 test call sites in `services/observation.rs`) all use `new_default()`, which creates a throwaway built-in claude-code registry per call rather than sharing the startup-configured registry Arc.

The `parse_observation_rows` function signature correctly accepts `_registry: &DomainPackRegistry`, but the registry parameter is never consulted — the underscore prefix marks it unused.

**Impact for W1-5**: Functional behavior is correct because:
- The hook ingress path hardcodes `source_domain = "claude-code"` (FR-03.3 — correct)
- `new_default()` creates a registry with only the built-in claude-code pack
- For W1-5 (no external domain packs in production), `new_default()` is equivalent to the injected registry

**Impact for external packs**: If an operator configures `[[observation.domain_packs]]` in TOML, the configured registry (with category registrations applied to `CategoryAllowlist`) is built at startup, but the retrospective pipeline's `SqlObservationSource` instances use the throwaway default registry. The category side-effect (CategoryAllowlist registration) IS applied correctly. The event resolution side-effect is not.

**Agent documentation**: The ingest-security agent report explicitly acknowledges this gap: "The `_observation_registry` in `main.rs` is not currently plumbed to individual request handlers — this is intentional for W1-5 since all events are hook-path and always resolve to `source_domain = 'claude-code'`."

**Issue**: Pseudocode specifies the wiring; implementation defers it with an intentional design note. The IR-01 integration test requirement ("assert that with the default built-in claude-code pack, event_type = 'PreToolUse' resolves to source_domain = 'claude-code'") IS satisfied behaviorally but not via the architecture-specified mechanism.

**Fix**: Rename `_observation_registry` to `observation_registry` and pass `Arc::clone(&observation_registry)` to `SqlObservationSource::new()` at all `context_cycle_review` and related retrospective call sites (at minimum `mcp/tools.rs` and `services/status.rs`). The `new_default()` convenience constructor can remain for lower-priority call sites (status checks, legacy paths) as documented.

---

### 2. Architecture Compliance

**Status**: PASS

All ADR decisions are implemented as specified:
- ADR-001: `ObservationRecord` uses `event_type: String` + `source_domain: String`; `hook_type` is a constants module, not an enum
- ADR-002: Config-only domain pack registration; no MCP write path
- ADR-003: `RuleEvaluator` handles threshold + temporal window only; `serde_json::Value::pointer` used for field extraction
- ADR-004: Four-wave compilation-gated refactor completed; `cargo check --workspace` passes after each wave (per agent reports)
- ADR-005: Mandatory `source_domain` guard is first operation in all 21 rules
- ADR-006: `UniversalMetrics` typed struct preserved; `domain_metrics: HashMap<String, f64>` added as extension
- ADR-007: 64 KB size check + depth-10 check applied at ingest boundary in `parse_observation_rows`

Schema v14 migration verified: `CURRENT_SCHEMA_VERSION = 14`; `ALTER TABLE observation_metrics ADD COLUMN domain_metrics_json TEXT NULL` with idempotency guard (checks column existence before applying).

---

### 3. Interface Implementation

**Status**: PASS

All interfaces match pseudocode definitions:

- `ObservationRecord`: `event_type: String`, `source_domain: String`, all other fields preserved — matches pseudocode OVERVIEW.md exactly
- `DomainPack`, `DomainPackRegistry`: match pseudocode; `lookup()`, `rules_for_domain()`, `resolve_source_domain()`, `iter_packs()` all present
- `RuleDescriptor` enum: `Threshold(ThresholdRule)` / `TemporalWindow(TemporalWindowRule)` — matches pseudocode
- `MetricVector`: gains `domain_metrics: HashMap<String, f64>` with `#[serde(default)]` for v13 compatibility
- `ObservationConfig` / `DomainPackConfig`: present in `config.rs` with `#[serde(default)]` on `ObservationConfig`
- `ObserveError` new variants: `PayloadTooLarge`, `PayloadNestingTooDeep`, `InvalidSourceDomain`, `InvalidRuleDescriptor` — all present (confirmed by grep in domain/mod.rs and evaluator.rs)

Error handling follows project pattern (`unwrap_or_else(|e| e.into_inner())` for RwLock poison recovery).

---

### 4. Test Case Alignment

**Status**: PASS

Test coverage against test plans:

**observation-record.md**: T-OR-01 through T-OR-04 present in `unimatrix-core/src/observation.rs` tests. All compile-time structural assertions in place.

**domain-pack-registry.md**: `crates/unimatrix-observe/tests/domain_pack_tests.rs` exists and covers T-DPR-01 through T-DPR-14 plus EC-04, EC-05, EC-07, EC-08, EC-09.

**rule-dsl-evaluator.md**: T-DSL-01 through T-DSL-18 covered in `domain_pack_tests.rs`. Source domain guard verified in `detect_threshold()` and `detect_temporal_window()` (ADR-005 step 1 as first operation in both).

**detection-rules.md**: All 21 rules present in `default_rules()`; test count 401 in `unimatrix-observe` (357 lib + 44 integration) per agent-6 report. `source_domain` guards confirmed as first operation in every `detect()` method across agent.rs (7 rules), friction.rs (4 rules), session.rs (5 rules), scope.rs (5 rules) — all reviewed.

**metrics-extension.md**: T-MET-01 through T-MET-05c present in `unimatrix-store/src/metrics.rs`. `UNIVERSAL_METRICS_FIELDS.len() == 22` asserted; original 21 field names verified by name; `domain_metrics_json` verified separately by name.

**schema-migration.md**: v13→v14 migration with idempotency guard present at `migration.rs:454-480`. `CURRENT_SCHEMA_VERSION = 14`.

**ingest-security.md**: 17 new tests in `services/observation.rs` covering T-SEC-01 through T-SEC-14. AC-06 payload boundary (exact 64 KB passes; 64 KB + 1 byte rejects), AC-06 depth boundary (depth 10 passes; depth 11 rejects), AC-11 unknown event passthrough, FR-03.3 hook-path always claude-code.

**R-03 test fixture audit**: All test fixtures in col-023 code supply both `event_type` and `source_domain`. Verified by file review of agent.rs, friction.rs, session.rs, scope.rs, domain_pack_tests.rs — no empty `source_domain: ""` construction for claude-code test records.

---

### 5. Code Quality — No Stubs or Placeholders

**Status**: PASS

No `todo!()`, `unimplemented!()`, or `FIXME` in col-023-introduced code. Two `// TODO(W2-4)` comments exist in `main.rs:610` and `main.rs:989` and `services/mod.rs:255` — these are pre-existing scope markers for a separate future feature (gguf_rayon_pool), not introduced by col-023.

---

### 6. Code Quality — File Size

**Status**: WARN

Files exceeding 500 lines touched by col-023:

| File | Pre-col-023 Lines | Post-col-023 Lines | Delta |
|------|------------------|--------------------|-------|
| `unimatrix-observe/src/detection/agent.rs` | 907 | 945 | +38 |
| `unimatrix-observe/src/detection/friction.rs` | 533 | 550 | +17 |
| `unimatrix-observe/src/detection/session.rs` | 632 | 660 | +28 |
| `unimatrix-observe/src/detection/scope.rs` | 614 | 644 | +30 |
| `unimatrix-observe/src/metrics.rs` | 991 | 1140 | +149 |
| `unimatrix-server/src/services/observation.rs` | 848 | 1280 | +432 |
| `unimatrix-server/src/main.rs` | 1319 | 1377 | +58 |
| `unimatrix-store/src/migration.rs` | 1302 | 1326 | +24 |

All eight files were already over the 500-line limit before col-023. The feature grew pre-existing large files rather than creating new violations. `services/observation.rs` grew by +432 lines (test suite expansion for the 17 new security tests). This is WARN not FAIL because col-023 did not introduce the initial violation.

---

### 7. Code Quality — Clippy

**Status**: WARN

`cargo clippy --workspace -- -D warnings` reports 18 errors, all in `unimatrix-store`:
- `analytics.rs:288` — `while_let_loop` (pre-col-023: not touched by this feature)
- `migration.rs:809` — needless `&data` deref (pre-col-023 line in v5→v6 legacy migration code; col-023 additions are at line 454-480)
- `read.rs:393,409` — collapsible `if` (pre-col-023)
- `write.rs:27,87,180,181,218,219,259` — needless borrows (pre-col-023)
- `write_ext.rs:340,346,421,439,445,548` — needless borrows (pre-col-023)
- `observations.rs:81` — too many arguments (pre-col-023)

None of these errors are in code introduced or modified by col-023's feature scope. These are pre-existing lints that were present at commit `80b879a` (the crt-023 commit immediately preceding col-023 work).

---

### 8. Security Checks

**Status**: PASS (cargo audit WARN due to tool absence)

- No hardcoded secrets, API keys, or credentials in col-023 code.
- Input validation enforced: 64 KB payload size + depth-10 JSON nesting checked in `parse_observation_rows`.
- `source_domain` validated against `^[a-z0-9_-]{1,64}$` at domain pack registration in `validate_config()`.
- `DomainPackRegistry` has no MCP write path; `new()` is startup-only (AC-08 structural assertion holds).
- DSL rule evaluation is pure data transformation; no filesystem access, no dynamic code.
- `cargo-audit` is not installed in this environment; CVE verification cannot be performed.

---

### 9. Knowledge Stewardship

**Status**: PASS

Agent-9 (ingest-security) report at `product/features/col-023/agents/col-023-agent-9-ingest-security-report.md` contains:
- `Queried:` entry for unimatrix-server patterns
- `Stored:` entry documenting the `new_default()` vs `new(store, registry)` design note as a pattern

The registry wiring gap is explicitly documented in the agent report as intentional for W1-5, making it a tracked conscious decision rather than an unnoticed omission.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| `_observation_registry` not threaded into `SqlObservationSource` at retrospective call sites | uni-rust-dev (Wave 4 follow-up) | In `main.rs` tokio_main_daemon and tokio_main_stdio: rename `_observation_registry` to `observation_registry`. Pass `Arc::clone(&observation_registry)` to `SqlObservationSource::new()` in the `context_cycle_review` and related MCP tool handlers (`mcp/tools.rs:1115`, `mcp/tools.rs:1368`) and in `services/status.rs:715`. Preserve `new_default()` for lower-priority status/stats call sites where W1-5 behavior is acceptable. Add an integration test asserting that a TOML-configured domain pack's `source_domain` is visible at the retrospective call site (AC-05 partial coverage). |

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for gate-3b registry wiring patterns — no prior entry for `DomainPackRegistry` injection at MCP request handlers. This is the first time this pattern has been gated. If it recurs in a future feature, warrant a lesson-learned entry.
- Stored: nothing novel to store — the registry injection gap is feature-specific and documented in the agent report. A recurring pattern entry would be warranted if two or more features exhibit "startup-built Arc not threaded into request handlers."
