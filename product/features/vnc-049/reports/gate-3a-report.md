# Gate 3a Report: vnc-049

> Gate: 3a (Component Design Review)
> Date: 2026-09-15
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | C1–C9 map 1:1 to the ARCHITECTURE component breakdown; interfaces match the Integration Surface; ADR-001..009 honored in pseudocode. |
| 2. Specification coverage | PASS | FR-01..12 all have corresponding pseudocode; NFR-01..06 addressed; no scope additions. |
| 3. Risk coverage | PASS | R-01..R-17 all mapped to test scenarios; the four demanded authoritative non-proxy assertions are designed in. |
| 4. Interface consistency | WARN | Two cross-artifact inconsistencies with pinned resolution paths: model_id charset (pseudocode vs test-plan) and parity-corpus target file (pseudocode c8 vs test-plan c8). |
| 5. Integration harness plan | PASS | test-plan/OVERVIEW §4 present with 8 infra-001 tests, anti-seed contract, suite table, and a MANDATORY smoke gate (§4.5). |
| 6. Knowledge stewardship | PASS | Both agent reports carry a `## Knowledge Stewardship` block with Queried entries; testplan has a Stored "nothing novel -- {reason}"; pseudocode is read-only (Queried + "no storage"). |

## Detailed Findings

### 1. Architecture alignment
**Status**: PASS
**Evidence**: Every component in `pseudocode/OVERVIEW.md` maps to the ARCHITECTURE §Component
Breakdown table (C1 plugin shim, C2 Rust arm + new `hook/opencode.rs`, C3 JS mirror, C4 wire carriers,
C5 ingest persistence, C6 read path, C7 domain pack, C8 parity corpus, C9 installer). Integration
Surface names are reused verbatim, not invented: `KNOWN_PROVIDERS` @hook.rs:158, `HookInput`/`ImplantEvent`
wire fields, `insert_observation:3383`/`insert_observations_batch:3416`, `parse_observation_rows:576`,
`DEFAULT_HOOK_SOURCE_DOMAIN:572`, `resolve_source_domain:180`, `DomainPack`. C5/C6 pseudocode
precisely implements ADR-001 (opencode-only ingest stamp, prefer-stored read fork, fail-loud
`debug_assert` canary). AC-06 carried in-cycle per ADR-003. New-module-with-thin-wiring (ADR-005)
applied for `hook/opencode.rs` and `opencode-install.js`; over-cap files receive wiring lines only.

### 2. Specification coverage
**Status**: PASS
**Evidence**: FR→pseudocode: FR-01/02/03/12→C1; FR-04/05→C2+C3+C8; FR-06→C5(write)+C6(read)+C7(legacy
fallback); FR-07→C1 (observe-channel subagent alignment); FR-08→C4+C5+C6 (model_id carrier→persist→
surface); FR-09/10→C9 (non-clobber additive installer). FR-11 (AC-07 seam) is correctly left untouched
and kept viable, consistent with the spec's "no enforcement, no N=1 ceremonial test" (SR-12). NFR-01
(stored-record verification) is the c5 assembled-path contract; NFR-02 (fail-open) is C1's error
handling; NFR-03 (retrieval non-regression) is C9/AC-05c; NFR-04 (parity) is C8; NFR-05 (cap) is
ADR-005 posture; NFR-06 (idempotence) is C9. No unrequested features introduced.
**Minor**: The AC-07 "external_identity seam remains callable" static assertion is not assigned to a
named component test plan; its spoof-rejection half (R-07) is covered in c1/c8. This is consistent with
SR-12's explicit prohibition of a ceremonial N=1 seam test, so it is not a defect — noted for the
tester to confirm the structural half is exercised (or documented as deliberately not tested) at 3c.

### 3. Risk coverage
**Status**: PASS
**Evidence**: test-plan/OVERVIEW §2 maps every R-01..R-17 to a discharging scenario and proof surface.
The four assertions this gate specifically demands are all designed in as non-proxy:
- **AC-03** — c5 `test_opencode_event_stored_source_domain_is_opencode` + MANDATORY negative
  `test_opencode_event_stored_source_domain_not_claude_code`, asserted on the SELECTed row from an
  assembled dispatch→insert→read-back following `listener/tests/foreign_domain.rs`. OQ-4 site pinned
  via a #5427 per-site behavioral matrix (`test_source_domain_produced_at_pinned_ingest_site`).
- **AC-06 (C18 GATING, R-01)** — c5 `test_opencode_local_model_event_stored_and_queried_distinct_from_cloud`
  asserts distinctness on the QUERIED row (not in-memory `ImplantEvent`); anti-tautology negative
  `test_opencode_model_id_dropped_between_wire_and_insert_fails` is mandatory; two-local-models
  mutual distinctness present. Seeding and `.is_some()`/flag-passed proxies are explicitly forbidden.
  Marked GATING; C18 stays `partial` until green.
- **R-05 opencode-only stamp** — c5 `test_source_domain_stamp_fires_only_for_opencode` (per-provider
  write matrix) + explicit "T-SEC-12/13 stay green unchanged" keyed to exact anchors
  (`test_parse_rows_unknown_event_type_passthrough`:1360, `test_parse_rows_hook_path_always_claude_code`:1390).
- **Lesson #5670** — the c5 R-01 crossing test IS the client→listener→DB model_id crossing test
  (drives production insert, not a helper); c8 carries the model_id parity-corpus case. Both
  cross-referenced.
Degraded/experimental legs (R-11 Stop over-count, R-12 PreCompact) are covered in c1 with the
ADR-009 fail-safe/non-cap-failing posture; parity-gap honesty (R-16) covered in c1+c8.

### 4. Interface consistency
**Status**: WARN
**Evidence / Issue**:
- **model_id charset mismatch.** All pseudocode (c1:81, c2:68, c3:42, c4:50, OVERVIEW open-questions)
  correctly defines the model_id carrier charset as `^[a-z0-9._/-]{1,128}$` (must allow `/` for
  `ollama/qwen3-coder`). The test plans c1 (:82), c4 (:38), and c5 (:104) still describe model_id
  validation with the `source_domain` charset `^[a-z0-9_-]{1,64}$`, inherited from RISK-TEST-STRATEGY
  R-15. A literal test against `^[a-z0-9_-]{1,64}$` would reject the very value AC-06 depends on.
  Not blocking (pseudocode pins the correct resolution; the AC-06 distinctness test itself uses a
  valid `/`-bearing id), but delivery must reconcile the model_id validation tests to the
  `/`-allowing charset before writing them. Fix at Stage 3b.
- **Parity-corpus target file.** c8 pseudocode names `uds/parity_corpus_uds.rs`; the c8 test plan
  (verified against tree) corrects this to `uds/parity_corpus_cases*.rs` + `uds/parity_corpus_gen.rs`
  (`assert_coverage`@gen.rs:363, `all_arm_keys()`@gen.rs:92, `scripts/check-parity-drift.sh`);
  `parity_corpus_uds.rs` is UDS framing/byte goldens only. c2 test plan (:48) also still references
  the wrong file. Flagged as an open question in both OVERVIEWs with the empirically-correct seam
  pinned in the test plan. Delivery confirms the receiving file at 3b.
- Otherwise consistent: OVERVIEW shared types (HookInput, ImplantEvent, ObservationRow, observations
  table, DomainPack, KNOWN_PROVIDERS) match per-component usage; the write→read data flow is coherent;
  the dual-`ObservationRow` ambiguity (OQ-A) is correctly disambiguated — the write-path struct gains
  the fields in c5, the read struct (observations.rs:14) in c6.

### 5. Integration harness plan + smoke gate
**Status**: PASS
**Evidence**: test-plan/OVERVIEW §4 defines the infra-001 harness plan: §4.1 reachability (UDS-observe
via `UnimatrixHookClient`, GH#819 `daemon_server`/`_observation_row_count` model), §4.2 suite-selection
table, §4.3 the 8 new integration tests (extending existing fixtures, cumulative), §4.4 existing
regression sentinels (T-SEC-12/13, restart-persistence), and a load-bearing **anti-seed contract**
(#5285 — derive over the wire, never seed the row / inject the struct field; `FakeHookClient` is
explicitly disallowed for AC-03/AC-06). §4.5 states the MANDATORY minimum gate:
`python -m pytest suites/ -v -m smoke --timeout=60`, plus full-workspace unit + LINK smoke as 3c entry
gates.

### 6. Knowledge stewardship compliance
**Status**: PASS
**Evidence**: `vnc-049-agent-1-pseudocode-report.md` (read-only agent) has a `## Knowledge Stewardship`
block with `Queried:` entries (#5737, #4298, #4305, #5748, #5739, ...) and "Read-only tier; no storage
performed." `vnc-049-agent-2-testplan-report.md` has `Queried:` entries (#5748, #5737, #4373, #5427,
#5285, #4177/#3548, #378/#4153) and `Stored: nothing novel to store -- {reason}` with a reason. Both
compliant.

## Stage 3a Open Questions — disposition (none blocking)

| OQ | Design defect? | Resolution path (pinned) |
|----|----------------|--------------------------|
| OQ-A dual ObservationRow | No | Write-path struct gains fields in c5; read struct (observations.rs:14) in c6; delivery confirms exact construction site. |
| model_id charset | No (WARN) | Pseudocode pins `^[a-z0-9._/-]{1,128}$`; test-plan model_id validation tests must adopt it (see Check 4). |
| C7 EC-07 collision | No | c7 pins the safe form: do NOT claim shared canonical event_types, do NOT use empty (=claims-all); delivery picks explicit non-claiming form; c7 test #2 guards non-determinism. |
| Parity-corpus target file | No (WARN) | test-plan c8 pins the correct files/anchors; delivery confirms at 3b (see Check 4). |
| Schema version | No | Resolve `NEXT = CURRENT_SCHEMA_VERSION + 1` live at delivery (tree reads 31→next 32; BRIEF/ARCH "32" is off-by-one); do NOT hardcode. |
| OQ-3 session.idle→Stop | No | ADR-009 over-count guard; conservative dedupe until delivery-time PoC pins exact assertion. |
| OQ-7 PreCompact flag | No | ADR-009 degraded/flagged leg; delivery decides flag default; fail-safe designed. |

All open questions carry a pinned resolution path and are safely deferrable to implementation-time.
None constitutes a blocking design defect.

## Rework Required

None (PASS). Two reconciliation items for Stage 3b delivery (not gate-blocking):
1. Align the model_id validation test assertions in test-plan c1/c4/c5 to the `/`-allowing carrier
   charset `^[a-z0-9._/-]{1,128}$` (uni-tester / uni-rust-dev).
2. Confirm and correct the parity-corpus target file to `parity_corpus_cases*.rs`/`parity_corpus_gen.rs`
   in c8 (and c2 cross-ref) pseudocode (uni-pseudocode / delivery).
