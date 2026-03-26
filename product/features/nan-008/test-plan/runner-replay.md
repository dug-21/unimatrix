# Test Plan: runner/replay.rs

## Component Responsibility

Loads scenarios from JSONL, calls the service layer per profile, assembles
`ProfileResult`, and writes output. `run_single_profile` gains a new
`configured_categories: &[String]` parameter. After assembling the entries vec
(now including `category` populated from `se.entry.category`), it calls
`compute_cc_at_k` and `compute_icd`.

## Risks Covered

R-08 (ScoredEntry.category empty from mapping gap — integration-level catch),
R-09 (empty Vec from omitted TOML [knowledge] section), R-03 (empty categories
propagated from config).

---

## Integration-Level Tests

`replay.rs` is an async orchestrator that calls the live database. Its behavior
is tested at the integration level — either via `report/tests.rs` round-trip
(which exercises the serialized output) or via a fixture-based integration test
that runs `run_single_profile` against a test database.

Most `replay.rs` tests are indirect: the round-trip test in `report/tests.rs`
verifies that the fields populated by `replay.rs` survive the JSON round trip.
Direct tests for the replay path itself are limited to config wiring (R-09).

### `test_run_single_profile_populates_category_in_entries` (R-08)

This is the critical integration test for R-08. It requires a test fixture database
with entries spanning at least two categories.

```
Arrange: snapshot DB with entries in categories "decision" (>=1) and "lesson-learned" (>=1)
         configured_categories = ["decision", "lesson-learned", "pattern"]
         scenario = { query: "any query" }
Act:     result = run_single_profile(&scenario, profile, &configured_categories, ...).await
Assert:  result.entries.iter().all(|e| !e.category.is_empty())
         result.entries.iter().any(|e| e.category == "decision" || e.category == "lesson-learned")
         result.cc_at_k > 0.0
         result.icd > 0.0
```

If this test cannot use a fixture DB directly (due to async complexity), the
equivalent verification is done through the `run_report` round-trip test that
asserts `category` is non-empty in the JSON.

### `test_run_single_profile_configured_categories_passed_correctly` (R-09)

```
Arrange: profile TOML with [knowledge] section omitted; load config via standard path
Assert:  profile.config_overrides.knowledge.categories is non-empty
         // INITIAL_CATEGORIES must be present (7 values)
```

This is a unit test on the config loading path, not replay.rs itself. It verifies
the R-09 scenario: a profile that omits `[knowledge]` still provides a populated
categories vec.

---

## Config Unit Tests (R-09)

These tests live in a config test module, not in tests_metrics.rs:

### `test_knowledge_config_default_populates_initial_categories`

```
Act:     config = KnowledgeConfig::default()
Assert:  !config.categories.is_empty()
         config.categories.len() >= 7
         // Must include "decision", "convention", "lesson-learned", "pattern",
         // "procedure", "duty" (or their equivalents from INITIAL_CATEGORIES)
```

### `test_profile_omitting_knowledge_section_uses_defaults`

```
Arrange: toml = "[profile]\nname = \"test\"\n"  // no [knowledge] section
Act:     profile = toml::from_str::<EvalProfile>(toml)?
Assert:  profile.config_overrides.knowledge.categories is non-empty
```

---

## Wiring Assertions (compile-time)

The signature of `run_single_profile` must accept `configured_categories: &[String]`.
Any call site that omits this parameter fails to compile. The delivery agent must
update all call sites in `replay_scenario` (or equivalent) to pass
`&profile.config_overrides.knowledge.categories`.

No separate test is needed for this — compile success is the assertion.

---

## NFR Checks (code review)

- `replay.rs` remains fully async (`run_single_profile` is `async fn`)
- `configured_categories` is borrowed (`&[String]`), not cloned, unless a move boundary
  requires cloning (verify per ADR-001 SR-07 resolution in ARCHITECTURE.md)
- `se.entry.category` is the source of `ScoredEntry.category` — no hardcoded fallback
- `compute_cc_at_k` is called after the entries vec is fully assembled (not per-entry)
- `compute_icd` is called on the same entries vec
