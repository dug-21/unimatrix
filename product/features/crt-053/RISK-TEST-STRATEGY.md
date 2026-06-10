# Risk-Based Test Strategy: crt-053

**Feature**: Active-Only PPR Expansion Seeds — one surgical filter on `seed_ids` (`services/search.rs` Phase 0, inside `if self.ppr_expander_enabled`)
**GH Issue**: #717
**Inputs**: SCOPE.md (LOCKED), ARCHITECTURE.md, ADR-001, SPECIFICATION.md (AC-01..AC-05 + anti-AC), SCOPE-RISK-ASSESSMENT.md (SR-01..SR-06)
**Historical basis**: Unimatrix #4495 (vnc-018 scope-creep gate failure), #4888 (unmeasurability), #4902 (vacuous-pass trap), #4077 (direction-semantics spec trap), #724 (behavior-based ranking tests)

> This is a one-line production change in the most sensitive code in the system. The dominant risks here are NOT "the filter is wrong" — the filter is trivial. They are: (a) testing the wrong thing because effectiveness is unmeasurable (SR-01), (b) the filter dropping a legitimate active seed (SR-02), (c) the off-path drifting from bit-for-bit identity (C-02), and (d) the change quietly expanding beyond C-01 into the five locked exclusions (SR-03). Severity is anchored on "search quality regression" being the worst outcome the product can suffer.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Tester chases an unmeasurable metric gate (P@5/MRR/soft-GT) instead of behavior assertions; acceptance becomes vacuous or never closes (SR-01, #4888, #500 trap) | High | Med | Critical |
| R-02 | Filter over-drops — a legitimate active seed (6b terminal-active head or HNSW active) is excluded, silently shrinking expansion (SR-02) | High | Med | Critical |
| R-03 | Scope creep into the five locked exclusions (injection-side redirect, penalty-map extension, steepness, #585 edge hygiene, vnc-017 ceiling) — the exact vnc-018 failure (SR-03, #4495) | High | High | Critical |
| R-04 | AC-01/AC-05 absence assertion is vacuous — the "deprecated-only neighbor absent" arm passes because the neighbor was never reachable / the fixture lacks a positive out-edge, not because the filter excluded the seed (#4902 vacuous-pass) | High | High | Critical |
| R-05 | Off-path (`ppr_expander_enabled = false`) loses bit-for-bit identity via a shared helper, allocation, or struct mutation leaking outside the branch (SR-04, C-02) | High | Low | High |
| R-06 | Anti-AC violated — a test asserting deprecated *absence* in Flexible is added, contradicting the two-mode design (C-03) | Med | Med | High |
| R-07 | Test framed in reverse-walk / PPR power-iteration language asserts the wrong exclusion direction; `graph_expand` is forward BFS (seed B, edge B→X surfaces X) (SR-06, #4077, #3744) | Med | Med | High |
| R-08 | Fixture corpus (nan-018) lacks the ass-073 positive-edge revision, so AC-01/AC-05 cannot run on it and are silently skipped or under-tested (OQ-1) | Med | Med | High |
| R-09 | `results_with_scores` is not the sole seed source inside the enabled branch; another seed path bypasses the filter (OQ-2, FR-01 scope) | High | Low | High |
| R-10 | #406 reproduces in the delivery fixture and is "fixed" as a retrieval bug instead of raised as a fixture-divergence signal (SCOPE Disposition) | Med | Low | Medium |
| R-11 | Quarantine enforcement at `:950` is conflated with the seed filter and edited, breaking a security gate that is explicitly out of scope | Med | Low | Medium |
| R-12 | Predicate uses a string compare on a status field instead of the typed `Status::Active` enum, risking drift if enum repr/serialization changes (SR-02, FR-02) | Low | Low | Low |

Priority = Severity × Likelihood. Critical = High×(High|Med). High = High×Low or Med×Med. Medium/Low scale down.

## Risk-to-Scenario Mapping

### R-01: Unmeasurable effectiveness → metric-gate trap
**Severity**: High · **Likelihood**: Med · **Impact**: Acceptance is gated on a metric the platform cannot produce (#4888); the feature either never closes or closes on a vacuous/misleading number. Correctly demoting stale entries mechanically *drops* soft-GT P@5 — gating on it would reject a correct change (#500 trap, ass-073).

**Test Scenarios**:
1. Acceptance suite contains ZERO eval-harness metric gates (no P@5, MRR, soft-GT assertion) — verify by inspection of the test files added for crt-053.
2. Every acceptance assertion is over specific entry IDs (presence/absence/rank), runnable on the Python integration suite or nan-018 fixture corpus over raw `entries` JSON.

**Coverage Requirement**: All five ACs assert ID-level behavior only. No test imports or invokes the eval-harness scoring path as a pass/fail gate. (NFR-05.)

### R-02: Filter over-drops a legitimate active seed
**Severity**: High · **Likelihood**: Med · **Impact**: A 6b terminal-active head or an HNSW active is excluded → expansion silently narrows → current-knowledge neighbors that *should* surface do not. This is a quality regression that looks like "fewer results," easy to miss without a positive assertion.

**Test Scenarios**:
1. (AC-04) Fixture with a superseded chain whose terminal active head is 6b-injected: assert the terminal-active head anchors the walk and its active-only out-edge neighbor IS injected.
2. (AC-01 positive arm) An entry reachable via an *active* seed's out-edge IS present in the injected candidate set.
3. Mixed seed pool (HNSW actives + 6b heads + deprecated): assert every active seed's reachable neighbor is retained; only deprecated-only neighbors drop.

**Coverage Requirement**: At least one positive-presence assertion per active seed class (HNSW active, 6b terminal-active head). The filter must be proven to retain, not just to drop. (FR-03.)

### R-03: Scope creep into the five locked exclusions
**Severity**: High · **Likelihood**: High · **Impact**: The single most likely failure mode by precedent. vnc-018 inverted write-only PPR negative tests without an ADR and was caught only at product-owner review (#4495). Here the temptations are pre-named: `find_terminal_active` on injected entries, `penalty_map` extension, steepness, #585 edge hygiene, vnc-017 ceiling. Any of these makes search worse against no measurement (SR-01) while looking "more correct."

**Test Scenarios**:
1. Production diff touches exactly one file (`services/search.rs`) and exactly the `seed_ids` build inside the enabled branch — verify by diff review (C-01, FR-08).
2. No new symbol introduced: diff adds no `find_terminal_active` call on injected entries, no `penalty_map` mutation, no new config flag, no edge-write change.
3. Existing write-only negative tests for `graph_expand` relation types remain UNCHANGED (the #4495 trip-wire — inverting them is the documented failure).

**Coverage Requirement**: Diff-scope gate as an explicit review checklist item. The strategy treats "any of the five exclusions touched" as an automatic fail, not a judgment call. If something in scope looks wrong → raise, do not fix.

### R-04: Vacuous absence assertion (AC-01/AC-05)
**Severity**: High · **Likelihood**: High · **Impact**: The core acceptance test (`deprecated-only neighbor is absent`) can pass for the WRONG reason — the neighbor was never reachable, the deprecated seed had no positive out-edge, or the alias didn't resolve (#4902). A vacuous pass certifies a filter that may not actually be filtering.

**Test Scenarios**:
1. Control / mutation check: with the filter REMOVED (or deprecated seed forced active), the deprecated-only neighbor MUST appear — proving the absence in the real test is caused by the filter, not by unreachability. (Differential assertion.)
2. The deprecated seed in the fixture MUST have a verified positive out-edge to a neighbor that is reachable by NO other (active) path — assert the fixture's edge precondition explicitly.
3. B-present-while-A-absent symmetry: assert the active-seed neighbor IS present in the same fixture where the deprecated-seed neighbor is absent (both arms in one test, per #4902 truth table).

**Coverage Requirement**: AC-01 and AC-05 each include a differential/control arm proving the absence is filter-caused. No absence assertion stands alone. (SR-05.)

### R-05: Off-path identity drift
**Severity**: High · **Likelihood**: Low · **Impact**: If the filter touches a structure shared with the `ppr_expander_enabled = false` path, the default-off behavior changes — regressing the production-default config for a feature that should be inert there.

**Test Scenarios**:
1. (AC-02) With `ppr_expander_enabled = false`, search results (entries, order, scores) are identical to the pre-crt-053 baseline for the same fixture/query set.
2. Code review: the filter binding (`seed_ids`) is lexically inside the enabled branch and is referenced nowhere else; no shared helper, no struct field mutated.

**Coverage Requirement**: Default-off equivalence assertion passes AND the lexical-scope guarantee is confirmed by review. (C-02, FR-07, NFR-01.)

### R-06: Anti-AC violation (deprecated-absence-in-Flexible test)
**Severity**: Med · **Likelihood**: Med · **Impact**: A well-meaning test asserting deprecated entries are absent from Flexible search results contradicts the two-mode design and would force a future wrong "fix." This is the SR-03 family expressed as a test smell.

**Test Scenarios**:
1. (AC-03) Deprecated entries STILL appear in Flexible results and are still penalized (ranked below a comparable active on the penalized path).
2. Review gate: no test added for crt-053 asserts deprecated *absence* from Flexible.

**Coverage Requirement**: A positive presence-of-deprecated-in-Flexible assertion exists; the forbidden absence assertion is confirmed absent. (Anti-AC, C-03.)

### R-07: Reverse-walk direction mis-framing
**Severity**: Med · **Likelihood**: Med · **Impact**: `graph_expand` is forward BFS over Outgoing positive edges (seed B with edge B→X surfaces X), but the PPR power-iteration code uses `Direction::Outgoing` to mean a reverse walk (#3744), and prose conflating the two is a documented post-merge correction (#4077). A test asserting exclusion in the wrong direction validates nothing.

**Test Scenarios**:
1. Fixture edge directions are explicit: deprecated seed A has edge A→X (X reachable only forward from A); assert X absent. Active seed B has edge B→Y; assert Y present.
2. Verification is by entry presence/absence outcome only — never by inspecting a `Direction::` enum value in the test. (SR-06.)

**Coverage Requirement**: Every seed-exclusion test states its edge direction concretely and asserts on neighbor IDs, not on traversal internals. (FR-05.)

## Integration Risks

- **R-09 — seed-source completeness.** FR-01 assumes `results_with_scores` (post-6a/6b) is the *sole* seed source for `graph_expand` inside the enabled branch (confirmed at `:915`, OQ-2). If any other code path contributes seeds to the same BFS, the filter is incomplete. **Scenario**: assert (by review + a test that exercises a query producing 6b injections) that no un-filtered seed reaches `graph_expand`.
- **6a→6b→Phase 0 ordering.** The filter runs after 6a (penalty_map) and 6b (terminal-active injection) are complete. **Scenario**: AC-04 confirms 6b heads are present at filter time and survive — proving ordering holds.
- **graph_expand signature stability.** `graph_expand(&TypedRelationGraph, &[u64], depth, max) -> HashSet<u64>` is unchanged; only a narrower slice is passed. **Scenario**: existing graph_expand unit tests pass untouched (R-03 trip-wire #3 doubles as this check).

## Edge Cases

- **All seeds deprecated** → `seed_ids` empty → BFS runs over zero seeds → no neighbors injected. Assert: no panic, no PPR injection, search still returns HNSW + 6b results. (Empty-seed boundary.)
- **No deprecated seeds present** → filter is a no-op; injected set identical to pre-filter. Assert: parity with unfiltered behavior on an all-active fixture.
- **Superseded-but-still-Active entry** → per spec domain model, `status == Active` so it is RETAINED as a seed (the discriminator is status, not the `superseded_by` field). Assert: an Active entry with `superseded_by` set still anchors expansion. (Common misread of "superseded.")
- **Proposed / Quarantined seeds** → dropped by `== Active` alongside Deprecated. Assert at least one non-Deprecated non-Active status is excluded, proving the predicate is `== Active`, not `!= Deprecated`. (R-12 / FR-02.)
- **6b head whose own out-edge neighbor is deprecated and reachable only via the >50-edge redirect ceiling** → knowingly NOT redirected (Locked Decision 4/5). Assert nothing here — this is accepted residual, NOT a test target. Documented to prevent a tester writing a failing test for it.

## Security Risks

This change accepts no new external input — the predicate reads an already-loaded, in-memory `EntryRecord.status` enum on candidates already admitted by HNSW. No new attack surface (no path, no query param, no deserialization). Blast radius is bounded to the seed list of one optional, default-off pipeline branch.

- **Quarantine gate must not regress (R-11).** `SecurityGateway::is_quarantined` at `:950` is a per-expanded-entry security check, separate from and downstream of the seed filter. **Risk**: a delivery agent conflates "filter Quarantined from seeds" with the existing quarantine enforcement and edits `:950`, weakening a security gate. **Scenario**: assert the `:950` quarantine check is unchanged in the diff; assert a Quarantined entry is still excluded by enforcement (not only by the new seed predicate). The seed predicate dropping Quarantined seeds is defense-in-depth, NOT a replacement for `:950`.
- **No status-spoofing path.** `Status` is `#[repr(u8)]` typed; the predicate cannot be bypassed by a malformed string. (FR-02 closes the only theoretical injection vector.)

## Failure Modes

| Failure | Expected behavior |
|---------|-------------------|
| `seed_ids` empty after filter (all candidates deprecated) | No panic; BFS over empty seeds returns empty neighbor set; HNSW + 6b results returned normally |
| Fixture lacks ass-073 positive-edge revision (R-08) | Tester routes AC-01/AC-05 to the Python integration suite OR extends the fixture first — does NOT silently skip the AC (OQ-1) |
| #406 reproduces in delivery fixture (R-10) | RAISE as fixture-divergence signal vs ass-073's eval graph; do NOT patch retrieval (SCOPE Disposition) |
| Something in scope looks wrong (R-03) | STOP and raise to product owner; do NOT fix — the no-adlibs directive is binding |
| Off-path result differs from baseline (R-05) | Treat as a C-02 violation / gate failure — the off path must be bit-for-bit identical |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (effectiveness unmeasurable) | R-01 | Architecture mandates behavior-based validation only (NFR-05); R-01 scenarios forbid any eval-harness metric gate. Accepted, mitigated by ID-level assertions. |
| SR-02 (must keep 6b heads, drop deprecated) | R-02, R-12 | Predicate is typed `Status::Active`; 6b heads pass by construction (`:814` guard). R-02 requires positive-retention assertions per active seed class; R-12 forbids string compare. |
| SR-03 (scope creep into locked exclusions) | R-03, R-06 | Locked Decisions carried verbatim into ARCHITECTURE.md; C-01 single-file boundary is structurally hard to cross. R-03 diff-scope gate + R-06 anti-AC gate enforce it. Precedent #4495. |
| SR-04 (off-path bit-for-bit identical) | R-05 | Filter lexically inside `ppr_expander_enabled` branch; lexical-scope guarantee (ARCHITECTURE "Off-Path Equivalence"). R-05 = AC-02 + review. |
| SR-05 (supersession false-positive guard) | R-04, R-02 (AC-04/AC-05) | Mandated fixture: Deprecated A superseded_by Active B, both with positive out-edges. R-04 adds a differential/control arm so the absence is provably filter-caused, not vacuous (#4902). |
| SR-06 (graph direction mis-description) | R-07 | Behavioral contract stated with concrete forward-edge examples (seed B, B→X); verify by outcome, never by `Direction::` enum. R-07 enforces explicit edge directions in fixtures (#4077, #3744). |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-04) | 11 scenarios — incl. all 5 ACs, the differential control arm for AC-01/AC-05, and the diff-scope gate |
| High | 5 (R-05, R-06, R-07, R-08, R-09) | 8 scenarios — off-path equivalence, anti-AC presence check, direction-explicit fixtures, fixture-route decision, seed-source completeness |
| Medium | 2 (R-10, R-11) | 3 scenarios — #406 raise-not-patch, quarantine `:950` unchanged + enforcement intact |
| Low | 1 (R-12) | 1 scenario — non-Deprecated non-Active status excluded (proves `== Active`) |

**Acceptance is met when**: AC-01..AC-05 pass with R-04's differential control arms; the diff touches only the `seed_ids` build in `search.rs` (R-03); off-path is bit-for-bit identical (R-05); no eval-harness metric gate exists (R-01); the anti-AC is confirmed absent (R-06); and existing graph_expand/penalty tests pass untouched (R-03/R-07/integration).

## Knowledge Stewardship
- Queried: /uni-knowledge-search for risk patterns — found #4495 (vnc-018 scope-creep gate failure, the SR-03/R-03 precedent), #4888 (unmeasurability → SR-01/R-01), #4902 (vacuous-pass trap → R-04, directly elevated AC-01/AC-05 likelihood), #4077 + #3744 (direction-semantics traps → SR-06/R-07), #724 (behavior-based ranking tests → C-04).
- Stored: nothing novel to store — the cross-feature patterns this strategy relies on (scope-creep-without-ADR #4495, unmeasurable-heuristic #4888, vacuous-pass #4902, direction-semantics #4077) are already captured as patterns/lessons in Unimatrix. No new 2+-feature risk pattern emerged beyond restating these for crt-053.
