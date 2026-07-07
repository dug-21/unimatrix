# Scope Risk Assessment: vnc-045

`context_tag` — domain-agnostic in-place tag mutation + per-slug protected-tag value-hygiene. Scope-level risks only; the security posture is human-LOCKED, so these inform design, not scope.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | The "tags outside the content hash" invariant is load-bearing for the whole fast path. Direct `entry_tags` write bypasses `update()`, so any state DERIVED from tags (in-memory tag filters `graph_read_filter.rs`, caches, tag-derived edges/indices) is not refreshed unless the op touches it. A missed derived surface = stale reads or silent integrity drift. | High | Med | Architect enumerates every surface derived from `entry_tags` (read-path filters, hot caches, any index) and confirms direct write updates or invalidates each. Evidence: #4372 (schema-extension touches N surfaces at once), #3216 (serving path silently used `::default()` when Arc unthreaded). |
| SR-02 | `single_value` replace = DELETE-then-INSERT. If not one transaction, a crash/interleave mid-replace leaves the entry with ZERO `delivery:*` tags (lost status), and the cadence guard could reject the corrective re-write. | Med | Med | Spec: replace is atomic (single tx); prior+new logged together (AC-05). Cadence guard must not count the two halves of one replace as two mutations. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Evidence-binding delegated to the evaluating agent (SD-14). Platform enforces value SHAPE, not value TRUTH — any `Write` holder can set `delivery:proven` with no proof. The allow-list's presence can read as rigor it does not provide. | Med | Med | AC-09 tool-description text must state plainly: hygiene ≠ authorization ≠ evidence. uni-capability skill (consumer) must not assume platform-side proof enforcement. |
| SR-04 | Value hygiene is best-effort ONLY on the fast path. `context_correct` bypasses the allow-list by design (SD-5/SD-11), so a typo'd/invalid value (`delivery:provn`) CAN still enter the tag lane via the unchanged path. Read-side and the worked example may still encounter invalid values. | Med | Med | Document hygiene as fast-path-only; readers/evaluators must tolerate out-of-vocabulary values. Do not build downstream logic that assumes allow-list completeness. |
| SR-05 | Three carry-forward spikes are marked out-of-scope (context_correct learning-vector reset; `audit_write_count_since` dormancy; metadata-filter-bypass tenant isolation). The tenant-isolation one is "inert under 1-client:1-project" — an assumption, not a guarantee. | Med | Low | Confirm no cross-project retrieval is in flight before relying on the isolation carve-out. Flag if any becomes a blocker rather than a deferral. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | **Five coupled per-slug config sites (SD-13, item 5).** Strong recurrence history of one threading site being missed SILENTLY. Worse: the local-UDS/daemon path reads global config directly (`main.rs:~980`) while only the per-slug HTTP loop calls `resolve_slug_config` — so `protected_tags` may be silently inert on the daemon path. The build-enforced classification test catches an ABSENT key, not INCORRECT threading. | High | High | Architect produces an explicit per-site threading checklist AND decides daemon-path behavior deliberately (honor global `protected_tags`, or documented-inert). Tester must use a behavioral per-site matrix, not source-assertion counting. Evidence: #3216, #2398 (dsn-001 threading gaps), #5427 (string-count tests blind to threading), #5269 (daemon vs per-slug divergence). |
| SR-07 | The `protected_tags` hygiene check slots onto the `validate_outcome_tags` interception point (`tools.rs:895-898`). Risk of conflating two distinct policies (reserved outcome vocabulary vs config-driven protected prefixes) at one site. | Med | Med | Keep the two validations separated in code; do not let outcome-tag reserved-key logic leak into protected-prefix evaluation. |
| SR-08 | The per-`(entry, protected-namespace)` cadence guard is a NEW stateful anti-abuse primitive with no precedent. State-model choices (in-memory vs persisted, global vs per-slug, restart-reset) are unspecified and easy to get wrong. | Med | Med | Architect pins the state model explicitly and aligns it with `check_write_rate` semantics (in-memory, restart-reset, per-`CallerId`); decide per-slug scoping. |

## Dependency Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-09 | `merge_configs` is hand-written with no catch-all; list replace-vs-merge for `protected_tags` is called a "design choice" but has a security edge: a slug that MERGES rather than REPLACES could silently inherit base-config allow-list values it never declared. | Med | Med | Architect makes replace-vs-merge an explicit ADR with the inheritance implication stated; default to replace for a policy list unless a merge rationale is documented. |
| SR-10 | uni-capability `SKILL.md` (worked-example consumer) requires exactly one `delivery:` tag and encodes the value-set `{proven, partial, missing, asserted}`. If op behavior (single_value replace, allowed values) drifts from the skill's assumptions, the consumer breaks — and the value-set lives in config, not the op. | Low | Med | Ship the `delivery:` policy as an example config entry that matches `SKILL.md` exactly; note the skill depends on config, not op internals. |

## Assumptions

- **A1 (SCOPE §11 line 11, SD-1):** Tags are and remain outside the content hash and outside embedding input. If any future integrity/embedding path begins consuming tags, the fast path becomes integrity-unsafe. Basis for SR-01.
- **A2 (SCOPE §76, SD-13):** The `PerSlugOverlayable` policy is honored on the per-slug HTTP serving path; the single-project daemon path is assumed acceptable reading global config. Unstated whether daemon deployments need per-project hygiene. Basis for SR-06.
- **A3 (SCOPE §116):** Metadata-filter-bypass is inert "under 1-client:1-project." True only while no cross-project retrieval exists. Basis for SR-05.
- **A4 (SD-11):** Value hygiene need not be complete because `context_correct` can bypass it. Assumes no downstream logic treats the tag lane as validated. Basis for SR-04.

## Design Recommendations

1. **Enumerate the full `entry_tags` derived-state blast radius before designing the write** (SR-01) — treat this like the multi-surface audit pattern (#4372); a checklist, not ad-hoc.
2. **Make the per-slug threading a first-class design artifact** (SR-06): an explicit five-site checklist plus a deliberate, documented decision on daemon-path behavior. This is the highest-likelihood failure mode by historical precedent.
3. **Pin atomicity and the cadence-guard state model in the spec** (SR-02, SR-08) — replace-as-one-transaction and cadence-guard scoping are silent-corruption sources if left implicit.
4. **Write the accepted-seam disclaimers into the tool description** (SR-03, SR-04): hygiene ≠ evidence ≠ authorization; hygiene is fast-path-only. Prevents downstream over-trust.
5. **Make `merge_configs` replace-vs-merge an explicit ADR** (SR-09) with the allow-list inheritance implication stated.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for lesson-learned + risk patterns — found #4372 (multi-surface schema extension), #3216/#2398 (dsn-001 Arc/field threading gaps), #5427 (source-assertion tests blind to threading), #5269 (daemon vs per-slug divergence), #5165/#5209 (per-slug config threading ADRs).
- Stored: nothing novel to store — the recurring "multi-site threading silently misses a site" pattern is already captured (#4372, #3216, #2398, #5427); this feature is an instance, not a new pattern.
