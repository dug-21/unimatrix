# Scope Risk Assessment: crt-050

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `observations.input` is stored as `Value::String` (double-encoded) on hook-listener path, making `json_extract(input, '$.id')` return NULL for all hook-sourced rows silently (pattern #4221, Gap 1) | High | High | Architect must verify Gap 1 resolution in crt-049 merged code (`listener.rs`) before committing to pure-SQL approach; fallback is two-phase SQL+Rust extraction |
| SR-02 | `ts_millis` (ms-epoch) vs `ts` (s-epoch) unit mismatch — wrong lookback boundary produces a 1000× window error (too narrow or too wide), silently accepted by the query | High | Med | Architect must make the unit difference explicit in the store API signature or add an assertion; a documentation-only fix is insufficient |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | MRR ≥ 0.2788 hard gate (AC-12) is a behavioral regression check, not an improvement check — if explicit-read signal is sparse (low observation count in lookback window), the table may be noisier than `query_log`, not better, and could fail the gate | High | Med | Spec writer should add an AC covering minimum observation count threshold before the gate is considered meaningful; AC-11 observations-coverage diagnostic is a partial mitigation but not a gate |
| SR-04 | `query_log_lookback_days` rename (AC-10) with `serde(alias)` is backward-compatible for TOML configs but any config passed as struct literal in tests will fail to compile — scope understates the rename surface area | Low | Low | Architect should audit all test fixtures that construct `InferenceConfig` directly |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | `cycle_events → sessions` join for outcome weighting uses `sessions.feature_cycle = cycle_events.cycle_id`; if `feature_cycle` is NULL (sessions created before col-022) or if a cycle has no `cycle_phase_end` rows, the join silently produces zero outcome rows — must degrade to weight 1.0, not error | Med | Med | Spec writer should add explicit AC for NULL `feature_cycle` degradation path (AC-05 covers missing phase events but not NULL FK) |
| SR-06 | `infer_gate_result()` lives in `tools.rs` (server crate); calling it from the `PhaseFreqTable` rebuild path in `services/` or `store/` introduces a crate-internal coupling or requires extraction — scope does not name the module boundary | Med | Med | Architect must decide whether to inline the substring-match logic or extract `infer_gate_result` to a shared location before spec is written |
| SR-07 | `phase_category_weights()` accessor (Goal 4, AC-08) is described as `pub` on `PhaseFreqTable` in `unimatrix-server` — W3-1 (ASS-029) may need it from a different crate, requiring a visibility change that is deferred to W3-1; deferred visibility decisions are often forgotten | Low | Med | Spec writer should note the visibility decision as a tracked open item for W3-1, not silence it |

## Assumptions

- **Section "Constraints / Critical (Gap 1)"** assumes crt-049 (#539) fully resolves the double-encoding issue at the storage layer. The merged commits (eaed9428, 5a6850db) may resolve it only at the Rust extraction layer. If so, the SQL-side `json_extract` approach is invalid for hook-path rows and the entire Step 1 architecture changes.
- **Section "Proposed Approach / Step 2"** assumes `query_phase_freq_table` has exactly one call site. If any test helper or benchmark constructs a call directly, deleting the fn breaks compilation silently at test time.
- **Section "Background Research / Outcome Weighting"** assumes `cycle_phase_end.outcome` free-text strings are consistent enough for substring matching. If outcome strings were written with different capitalization or encoding across older cycles, the weight map will be incorrect for historical data with no diagnostic.

## Design Recommendations

- **SR-01 is the critical path blocker.** The architect must read `listener.rs` at the merged crt-049 HEAD and confirm whether `input` is stored as a JSON object or a double-encoded string before writing a single line of SQL. If double-encoding persists, a two-phase extraction is required (pattern #4221 documents the two-branch form).
- **SR-03 / sparse signal.** The architect should design the new `rebuild()` to count distinct `(phase, session)` observation pairs and surface that count as a diagnostic field, so the spec can gate on it. An MRR gate without a signal-quality floor is a weak safety net.
- **SR-06 / module boundary.** Resolve `infer_gate_result` placement before the spec is written. Embedding substring-match logic inline in two locations violates DRY and creates drift risk for future outcome vocabulary changes.
