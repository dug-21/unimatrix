# Agent Report: vnc-016-agent-2-spec

## Output

`product/features/vnc-016/specification/SPECIFICATION.md` — written.

## Key Decisions

1. **`rule_name` string confirmed from source**: `"dependency_on_deprecated"` — read directly from `DependencyOnDeprecatedRule::name()` at `crates/unimatrix-observe/src/detection/scope.rs:286`. Not inferred.

2. **JSON assertion path confirmed from `RetrospectiveReport` struct**: `response_json["hotspots"][i]["rule_name"]`. The `hotspots` field is declared as `pub hotspots: Vec<HotspotFinding>` in `types.rs:391` and serialized to JSON under the key `"hotspots"` (no rename). Verified against the `format="json"` code path in `tools.rs:2848–2872`.

3. **SR-02 made explicit as C-01**: The `feature_entries` analytics write path flows through `context_store` → `UsageService.record_access` → `record_feature_entries`. Confirmed in `services/usage.rs:208–245`. `feature_cycle` must be on the original `context_store` call for entry A; no back-fill exists.

4. **SR-03 promoted to hard constraint (C-02)**: `force=True` is specified as mandatory in both integration tests, not a recommendation.

5. **`StoreParams.feature_cycle` serde contract**: The field is `Option<String>` without `#[serde(default)]` (`tools.rs:143`). Both absent key and explicit `null` map to `Option::None`. Harness client must use the `if feature_cycle is not None` guard pattern (FR-03.2) — passing the key only when non-None is correct.

## Constraints Discovered from Source Not in SCOPE.md

- **C-04 minimum observation row count**: SCOPE.md says "at least one observation session" but also says "at least 20 observation rows via `_seed_observation_sql`". The function default is `num_records=20`, which is sufficient. Spec formalizes this as the minimum with `num_records=0` explicitly prohibited.
- **`context_correct` does not affect `feature_entries` for entry A**: The successor entry created by `context_correct` does not inherit the `feature_entries` row from entry A. The SQL query only looks up `fe.entry_id = ge.source_id` (the original deprecated entry). This is a precision point implementers reading the query in isolation could miss; captured as C-06.
- **`uds_client.py` exclusion is a design decision, not a scope omission**: Confirmed from OQ-02 in SCOPE.md and from code: `uds_client.py` has a different session-context flow through `context_cycle`.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned ADR-004 (id 4421, constructor injection for DependencyOnDeprecatedRule), vnc-001 ADRs, and test infrastructure patterns. No directly applicable specification patterns; proceeding on source-read evidence.
