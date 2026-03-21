# Scope Risk Assessment: col-023

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | JSON-pointer extraction DSL (serde_json `json_pointer`) is path-only — it cannot express temporal window rules (N events within T seconds) or count aggregations across records without custom host logic. The scope commits to "threshold + temporal window rules" but the chosen DSL is insufficient for temporal rules without adding a rule evaluator layer. | High | High | Architect must define exactly what operators the DSL supports and how temporal window state is managed. If `json_pointer` is insufficient, an evaluator struct is needed; its complexity boundary must be drawn before implementation. |
| SR-02 | The structural test R-03/C-06 enforces column-field alignment between `UniversalMetrics` and `OBSERVATION_METRICS`. Option B retains those 21 columns, but the scope also claims `MetricVector.universal` becomes a `HashMap<String, f64>` "at the logical level." If both representations coexist (typed struct + HashMap), serialization round-trip correctness across the v13→v14 boundary becomes a hidden contract that must be verified — not just the new `domain_metrics_json` column. | High | Med | Architect must decide: is `UniversalMetrics` the source of truth or is `HashMap<String, f64>`? Two live representations of the same data with independent serialization paths is a regression surface. |
| SR-03 | The scope states `source_domain` is always `"claude-code"` on the hook ingress path and that non-Claude-Code domains use a "different future ingress (out of scope for W1-5)." This means the only domain exercised at runtime is `"claude-code"`. AC-05 uses a synthetic test, not a real ingress. The multi-domain path has no production exercising — regression risk is deferred, not resolved. | Med | High | Architect should ensure the ingest abstraction is testable without a real alternate ingress, and that AC-05 synthetic coverage is sufficient for W3-1's dependency on this pipeline. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `HookType` is referenced in 25+ files (entry #2843) but SCOPE.md scopes the blast radius to "server-side processing and unimatrix-observe." If any of those 25 files are in crates that do not get updated atomically (e.g., integration tests, bench harnesses, engine crate), the workspace will not compile mid-refactor and the PR cannot be merged incrementally. | High | Med | Architect must map every `HookType` callsite and confirm all are inside the PR boundary. Wave-based refactoring with compilation gates (entry #377) is the proven approach — design the phase boundaries explicitly. |
| SR-05 | The Admin runtime re-registration path is defined as "extend an existing tool" but no existing tool is named. This is an unresolved design decision embedded as a non-goal. If the architect picks `context_enroll` or another existing tool, the tool's schema changes — which is a broader impact than a non-goal implies. | Med | Med | Spec writer should either pin the target tool name and define the schema delta before implementation, or defer the Admin override entirely to a follow-on. Ambiguity here risks spec-architecture mismatch (entry #723). |
| SR-06 | The OUTCOME_INDEX / `BaselineSet` claim of "no migration required" depends on `BaselineSet.universal` being `HashMap<String, BaselineEntry>` today. If any baseline data was written with the typed struct field names as keys, deserialization into a HashMap works only if the keys match the new HashMap key strings exactly. This assumption should be verified against actual stored data before committing to no migration. | Med | Low | Architect should confirm the current serialized key format in OUTCOME_INDEX rows before finalizing the no-migration claim. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | The `parse_observation_rows` `_ => continue` skip today means unknown event types never enter the detection pipeline. After the change, unknown types pass through with `source_domain = "unknown"`. Detection rules written for "claude-code" will receive these records and must cleanly no-op rather than panic or produce false findings. The lesson in entry #699 (silent data orphaning via hardcoded None on the hook path) applies here in reverse — silent pass-through can produce phantom findings if rules do not gate on `source_domain`. | High | Med | Detection rule rewrite must include explicit `source_domain` guards. Architect should specify the contract: rules for domain X must never fire on events from domain Y. |
| SR-08 | W3-1 (GNN training signal) is blocked on W1-5 detection rules being "fully functional for the generalized event schema" (PRODUCT-VISION.md). If W1-5 ships the rules in a degraded state (claude-code-only, with future domains unverified), W3-1's unblocking claim is nominal rather than real. | Med | Med | Spec writer should define what "fully functional" means for W3-1's gate: does W3-1 require multi-domain detection rules, or only that the pipeline accepts multi-domain events? |

## Assumptions

- **SCOPE.md §Background Research "Storage layer is already generic"**: Assumes `hook` column is TEXT with no constraint and `insert_observation` takes `&str`. If any migration ever added a CHECK constraint on this column, the no-migration claim fails. Low likelihood but unverified.
- **SCOPE.md §Resolved Decision 3 "source_domain set server-side"**: Assumes the hook CLI ingress is the only current source of observation events. If any test fixture or integration path inserts observations directly via the store API with typed `HookType`, those paths will break when `ObservationRecord.hook` is removed.
- **SCOPE.md §Phase 4 "MetricVector.universal stays as typed struct under Option B"**: Assumes the logical HashMap representation is purely an in-memory abstraction with no storage impact. If the architect implements a true HashMap storage path, R-03/C-06 enforcement may be circumvented rather than updated (SR-02).
- **SCOPE.md §Constraints "No new crate dependencies for sandboxed rule DSL"**: Assumes `json_pointer` is expressive enough. This assumption is false for temporal window rules (SR-01).

## Design Recommendations

- **SR-01**: Define the rule evaluator contract (what operators exist, how temporal state is held) before writing the domain pack spec. Lock the DSL surface area explicitly.
- **SR-02**: Choose one canonical representation for `MetricVector.universal` — typed struct or HashMap — before specifying the storage layer. Two representations invite divergence.
- **SR-04**: Apply wave-based refactoring with compilation gates (entry #377): enumerate all `HookType` callsites, partition into waves, and ensure each wave compiles before proceeding.
- **SR-07**: Require explicit `source_domain` guards in all detection rules as a spec-level constraint, not an implementation detail. Cross-domain false findings are silent and difficult to diagnose post-merge.
