## ADR-003: Raise `nli_informs_cosine_floor` Default from 0.45 to 0.50

### Context

`nli_informs_cosine_floor` was set at 0.45 when crt-037 introduced Phase 4b. The rationale
was that the NLI neutral score (guard 1 of `apply_informs_composite_guard`) would act as a
quality filter over the 0.45–0.50 band: candidates with low cosine similarity but high NLI
neutral score could still qualify.

After ADR-001 and ADR-002, the NLI neutral guard is removed. Phase 4b candidates pass the
composite guard based solely on temporal ordering and cross-feature checks. Without the NLI
filter, pairs in the 0.45–0.50 cosine band qualify purely on structural metadata — temporal
ordering and feature cycle separation — with no minimum semantic similarity gate beyond 0.45.

A cosine of 0.45 represents low semantic overlap. For knowledge entries that are topic-level
documents (ADRs, patterns, decisions), a cosine of 0.45 may connect thematically unrelated
entries that happen to share vocabulary (e.g., two entries both discussing "tick" behavior
in different subsystems). The NLI neutral score was intended to filter this band; without it,
the floor itself becomes the sole semantic threshold.

The SCOPE.md decision (D-01) is explicit: "Raising `nli_informs_cosine_floor` from 0.45 → 0.5
provides an equivalent structural filter that does not rely on a task-mismatched NLI score."

0.50 is the same value as `supports_candidate_threshold` (default). This aligns the Informs
floor with the Supports threshold at the same semantic level, which is a defensible baseline:
if a pair's cosine similarity meets the bar for Supports candidate consideration, it also
meets the bar for Informs candidate consideration.

The inclusive floor semantics (>= not >) are preserved at 0.50. A pair at exactly cosine
0.500 is included in Phase 4b. A pair at cosine 0.499 is excluded. This is consistent with
the AC-17/AC-18 invariants established in crt-037 — only the threshold value changes.

SCOPE-RISK-ASSESSMENT.md SR-02 notes that no empirical data on candidate counts at 0.45 vs
0.50 was cited in SCOPE.md. The implementor should run a pre-condition corpus measurement
(scan HNSW against active entries, count pairs in [0.45, 0.50) vs [0.50, 1.0) for entries
passing the category pair filter) before committing the default change. If the [0.45, 0.50)
band contains a substantial fraction of all qualifying pairs at current corpus size (>40%),
the spec writer should review before accepting 0.50 as default.

### Decision

The default `nli_informs_cosine_floor` is raised from 0.45 to 0.50.

Changes:
- `default_nli_informs_cosine_floor()` in `config.rs` returns `0.5_f32` (was `0.45_f32`).
- `InferenceConfig::default()` field `nli_informs_cosine_floor` is `0.5` (was `0.45`).
- No validation change: 0.5 satisfies the existing `(0.0, 1.0)` exclusive range check.
- The inclusive floor semantics (`>=`) are unchanged.

Test updates required:
- All tests asserting `config.nli_informs_cosine_floor == 0.45` must assert `== 0.50`.
- `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold` (AC-18): the test
  uses `cosine_in_band = 0.47` which falls below the new floor. The test scenario changes:
  cosine at exactly 0.50 is in Phase 4b (inclusive) and at the boundary of Phase 4 (strict >
  means 0.50 is not a Supports candidate). The test must be updated to use cosine = 0.50
  and verify: Phase 4b accepts it (>=), Phase 4 rejects it (> strict).
- `test_phase8b_no_informs_when_cosine_below_floor` (AC-17): update to use 0.499 (below
  new floor) and verify rejection.
- New test: `test_phase4b_cosine_exactly_at_new_floor_accepted`: cosine = 0.50 is accepted
  (inclusive floor at new default).

Operator note: deployments that have set `nli_informs_cosine_floor = 0.45` in config.toml
are unaffected — the per-project override takes precedence over the compiled default.

### Consequences

- Pairs in the 0.45–0.499 cosine band are no longer considered for Informs edges with the
  default configuration. This reduces the candidate pool compared to crt-037's original intent
  but eliminates low-confidence semantic connections that the NLI neutral score was providing
  signal on.
- The Informs edge floor aligns with the Supports threshold at 0.50, making the semantic
  interpretation consistent: both edge types require at least 0.50 cosine similarity.
- Group 3 graph enrichment (S1 tag co-occurrence, S2 vocabulary) will add edges via
  structural signals that do not depend on cosine similarity at all — the floor change
  affects only the HNSW cosine scan path, not future signal sources.
- The eval harness (MRR >= 0.2913) gates crt-039. Any regression from the floor change
  is caught by the eval gate before merge.
