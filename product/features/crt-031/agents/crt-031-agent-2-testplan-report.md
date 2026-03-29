# Agent Report: crt-031-agent-2-testplan
## Stage 3a — Test Plan Design

---

## Summary

Produced 6 per-component test plan files rooted in the RISK-TEST-STRATEGY.md for crt-031
(Category Lifecycle Policy + boosted_categories De-hardcoding). All 3 Critical risks and
all 27 ACs are covered.

---

## Files Produced

| File | Purpose |
|------|---------|
| `product/features/crt-031/test-plan/OVERVIEW.md` | Overall strategy, risk-to-test mapping, integration harness plan |
| `product/features/crt-031/test-plan/categories.md` | CategoryAllowlist module split + new methods |
| `product/features/crt-031/test-plan/config.md` | KnowledgeConfig extension + validate_config + merge_configs |
| `product/features/crt-031/test-plan/main.md` | main.rs wiring + main_tests.rs rewrite |
| `product/features/crt-031/test-plan/status.md` | StatusService + StatusReport field |
| `product/features/crt-031/test-plan/background.md` | maintenance tick stub + run_single_tick wiring |
| `product/features/crt-031/test-plan/eval-layer.md` | eval/profile/layer.rs + 6 literal removal sites |

Note: IMPLEMENTATION-BRIEF.md lists a `test-plan/test-infra.md` component but the
ARCHITECTURE.md collapses test infrastructure literal removal into Component 7. The
test infrastructure literal removal tests are covered in `eval-layer.md` (same file as
the related production code fix), consistent with how the architecture groups them.

---

## Critical Risk Coverage

### R-01: validate_config parallel-list collision

Addressed in `config.md` with:
- Mandatory pre-implementation grep step: `grep -rn 'KnowledgeConfig {' crates/`
- Full enumeration of required fixture pattern (zero BOTH lists)
- `test_validate_config_adaptive_error_isolated_from_boosted` (AC-25) — proves the correct
  error variant fires when boosted list is zeroed
- `test_validate_config_boosted_error_isolated_from_adaptive` — proves error ordering
- `test_validate_config_both_parallel_lists_zeroed_ok` — proves the canonical pattern works

### R-02: StatusService::new() 4 construction sites

Addressed in `status.md` and `background.md` with:
- Mandatory pre-implementation grep: `grep -rn "StatusService::new" crates/`
- Sites 3 and 4 (test helpers in status.rs): compile-time catch after signature change
- Site 2 (`run_single_tick`): grep verification that `CategoryAllowlist::new()` does NOT
  appear inside the function
- `test_status_service_compute_report_has_lifecycle` — runtime test that the Arc reaches
  `compute_report()` and produces non-empty `category_lifecycle`

### R-11: KnowledgeConfig::default() callers

Addressed in `config.md` and `main.md` with:
- Mandatory pre-implementation grep: `grep -rn "KnowledgeConfig::default()" crates/`
- `test_knowledge_config_default_boosted_is_empty` (AC-17) — regression guard
- `test_knowledge_config_default_adaptive_is_empty` (AC-27) — companion guard
- `test_serde_default_boosted_categories_is_lesson_learned` (AC-18 rewrite) — tests the
  serde path, not the Default path

---

## Integration Harness Plan Summary

**Mandatory gate**: `pytest -m smoke`

**Suites to run**: `smoke`, `tools`, `adaptation`

**New integration test planned**: One test added to `test_tools.py`:
`test_status_category_lifecycle_field_present` — validates `category_lifecycle` appears in
JSON status response with at least 5 entries. Added during Stage 3b or 3c when JSON format
is confirmed.

**Suites not needed**: `lifecycle`, `volume`, `security`, `confidence`, `contradiction`,
`edge_cases`, `protocol` — this feature changes no schema, no protocol, no security boundary,
no scoring logic.

---

## Open Questions

1. **`test_maintenance_tick_stub_logs_adaptive_categories`** (AC-10): `maintenance_tick` has
   many parameters, making direct unit test construction verbose. If the implementer finds it
   impractical to unit-test `maintenance_tick` directly, the test plan accepts: code review of
   the stub + the AC-11 grep for `TODO(#409)` + the `test_lifecycle_stub_no_lock_across_await`
   code review as the AC-10 coverage. The tester (Stage 3c) should decide which approach is
   practical given the actual implementation.

2. **`category_lifecycle` JSON format**: The test plan notes that JSON comparison should use
   `serde_json::to_value`, not raw string equality (I-03). The exact JSON key and structure
   (`Vec<(String,String)>` serializes as an array of 2-element arrays by default in serde_json)
   should be confirmed during Stage 3b before writing the integration test assertion.

3. **`test_merge_configs_adaptive_global_fallback`** (R-07 scenario 2): The merge_configs
   project-overrides-global semantics compare against `default.knowledge.adaptive_categories`.
   After the Default impl change, `default.knowledge.adaptive_categories == vec![]`. If the
   project config also has `vec![]`, the comparison is `vec![] != vec![]` which is false, so
   global wins. The test must set up project and global carefully to cover both win directions.

---

## Self-Check

- [x] OVERVIEW.md maps all 11 risks from RISK-TEST-STRATEGY.md to test scenarios
- [x] OVERVIEW.md includes integration harness plan — smoke + tools + adaptation suites,
      one new test planned for test_tools.py
- [x] Per-component test plans match architecture component boundaries
- [x] All 3 Critical risks (R-01, R-02, R-11) have comprehensive test expectations
- [x] R-01: every fixture pattern enumerated; canonical zeroed-lists pattern documented
- [x] R-02: all 4 construction sites addressed; silent run_single_tick risk has grep guard
- [x] R-11: pre-implementation grep is a named mandatory step; AC-17, AC-18, AC-27 all covered
- [x] Integration tests defined for the MCP-visible boundary (category_lifecycle in status JSON)
- [x] All output files in `product/features/crt-031/test-plan/`
- [x] Knowledge Stewardship section included

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 19 entries. Directly relevant: #3771
  (parallel list collision trap for validate_config fixtures), #3774 (Default/serde split silent
  test failure), #3770 (parallel list structural pattern), #238 (test infrastructure conventions),
  #3253 (gate verification grep pattern for test rewrites).
- Queried: `context_search(query="validate_config test fixture parallel list collision", category="pattern")` — found #3771, #3770, #3774 directly applicable to crt-031. Applied all three to test plan design.
- Stored: entry #3776 "Test plan for parallel config list fields: grep fixtures first, add
  isolation test per list" via context_store — this captures the test-plan-design procedure
  (grep-first, isolation-test-required) as distinct from entry #3771 which captures the
  runtime trap. The two entries are complementary: #3771 tells implementers what breaks,
  #3776 tells test plan agents how to prevent it structurally.
