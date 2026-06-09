# ADR-004 (nan-018): Property-Based Ground Truth + Reusable Trust Assertion Class, via Corpus Aliases

### Context

The harness rewards *presence* of ground-truth IDs (`expected: Option<Vec<u64>>`, literal IDs). It has **no** trust/negative metric: "the stale source is absent" and "deprecated ranks below weakest active" are inexpressible. crt-053's steepness target is a **sweep**, so its success criterion (trust holds) must be measurable **in the same instrument** alongside P@5/MRR (C-03) — routing trust to a separate Python suite cannot correlate "this steepness → trust holds AND P@5 didn't regress."

Separately, literal-ID ground truth goes stale on every re-snapshot, and null-`expected` self-consistency burned ASS-039/ASS-037 (#3997). The durability bet (C-04, SR-07) is **property-based** assertions — but if properties are under-specified they regress to literal-ID or null behavior, the exact rework that burned before. SR-06: the trust "class" is scoped reusable (quarantine, contradiction) but generality ambition can balloon Wave-1.

### Decision

**1. Additive, alias-resolved assertion structure — kept separate from `expected`.**

`ScenarioRecord` gains `assertions: Option<ExpectedAssertions>` (additive; `expected` stays for wire-compat; log-sourced scenarios never set `assertions`):

```rust
pub type EntryRef = String;  // a corpus alias, e.g. "chainA.head"

pub struct ExpectedAssertions {
    pub redirect_to_head: Vec<EntryRef>,         // queried member must surface its terminal active head
    pub forbidden_absent: Vec<EntryRef>,         // must NOT appear in results
    pub rank_below:       Vec<(EntryRef, EntryRef)>, // A ranks strictly below B (or A absent)
}
```

Assertions are authored against **corpus aliases**, never literal IDs (C-04). The fixture loader (ADR §corpus) returns an `alias → entry_id` map; assertions resolve at load time. Re-snapshotting reassigns IDs but aliases are stable, so assertions survive shape changes that literal IDs do not (SR-07). **No null `expected` and no literal-ID `expected` in the primary corpus** — banned by a corpus-validation check that rejects a primary fixture carrying `expected: Some(Vec<u64>)` or carrying neither `expected` nor `assertions`.

**2. Operational definitions (SR-07 — each property is defined, not hand-wavy):**

- **redirect-to-head** — given a query that targets a superseded/deprecated chain member, the chain's terminal active head (resolved by the same `find_terminal_active` semantics the engine uses) MUST appear in the result list at a rank ≤ the queried member's rank (head surfaces at-or-above the stale member). Fails if head absent or ranked below the member.
- **absence** — the resolved id for the alias MUST NOT be present in the returned `entries`. Shared with the trust class.
- **rank-below `(A, B)`** — both resolved; if A present and B present, `rank(A) > rank(B)` (strictly lower). If A absent → pass (absent is "below everything"). If B absent and A present → fail (A cannot be below a missing B). Shared with the trust class.

**3. Trust as a class, evaluated in-harness.** `eval/runner/trust.rs::evaluate_trust(entries: &[ScoredEntry], assertions: &ExpectedAssertions, alias_map: &AliasMap) -> TrustOutcome`:

```rust
pub struct TrustOutcome {
    pub absence_pass: bool,   // all forbidden_absent satisfied
    pub rank_pass: bool,      // all rank_below satisfied
    pub violations: Vec<String>, // human-readable per-failure detail
}
```

Called in `run_single_profile` immediately after the `ScoredEntry` list is built, **per profile, per scenario**, and stored on `ProfileResult.trust`. Because it is per-profile it appears **alongside** P@5/MRR for every config in the sweep (AC-04). `find_regressions` is extended (same OR semantics as the existing `mrr < baseline || p_at_k < baseline`): a candidate that flips `absence_pass` or `rank_pass` from true→false is a regression and is surfaced + counted (AC-02/03). `redirect_to_head` is evaluated the same way and contributes to the relevance side (it is a positive property).

**4. Extensibility without speculation (SR-06).** `ExpectedAssertions` holds `Vec`s; the evaluator is a straight-line check per vector. Future correctness properties (quarantine-absent, contradiction-suppressed) add a new `Vec` + a new check arm — no call-site change. **Wave-1 ships exactly the three property types above and nothing speculative.**

**5. Corpus authoring depth — beyond the AC-14 minimum (steepness-crossover findability).** The simplified chain (nan-018 → ass-073 → crt-053) dropped ass-073's original fixture-feasibility probe, so nan-018 now authors the Wave-1 corpus **cold**. AC-14's non-vacuous gate only proves the corpus *measures something* (≥ 1 trust assertion evaluated against a non-empty result set) — it does **not** prove the corpus is a good-enough yardstick for crt-053's Q8 (the steepness question). Therefore the Wave-1 corpus MUST be authored **beyond the AC-14 floor**: it must carry enough variation — **especially in the deprecated-but-connected shape** — that the **steepness crossover is findable**, i.e. the sim/conf point at which a connected-deprecated entry crosses the weakest-active threshold can be located within the corpus's spread. A corpus authored only to the AC-14 minimum would force ass-073's steepness answer to rest on weak evidence (a single point, no crossover bracket). Concretely, the deprecated-but-connected fixture family must span a range of (similarity, connection-strength) points bracketing the expected crossover, not a single exemplar. This requirement flows to the **Band-2 authoring-guide spec** as an explicit authoring obligation.

### Consequences

**Easier:** "Steepness X → trust holds AND P@5 didn't regress" is one correlated row in one run (AC-04, AC-14). Property assertions survive re-snapshot (SR-07), so the primary corpus stays valid across shape changes the literal-ID approach could not. The corpus-validation ban makes the ASS-039 null-`expected` trap a compile-checked-at-load failure, not a silent regression. The same `absence`/`rank-below` primitives serve both the trust class and the corpus property layer — one evaluator, two consumers.

**Harder:** Authors must think in properties + aliases, not IDs — the Band-2 authoring guide must teach this explicitly or authors will reach for literal IDs (the SR-07 failure mode). The alias→id resolution adds a load-time step and a new failure mode (unknown alias → load error, which is the desired loud failure). Trust assertions are only as good as the fixtures that exercise the five status shapes (AC-06) — a too-small corpus proves the instrument runs but not that it measures (SR-06 residual, mitigated by the AC-06 minimum shape coverage).

**Named revision loop (anticipated, not a failure):** the Wave-1 corpus is **not frozen**. "ass-073 finds the Wave-1 corpus insufficient for the steepness crossover → revise nan-018's corpus" is a **valid, expected loop**, not a surprise or a defect. Because nan-018 authors the corpus cold (no upstream feasibility probe), the depth requirement (Decision §5) reduces but does not eliminate the chance ass-073 needs more bracketing points. When ass-073 signals insufficiency, the corpus is revised (more deprecated-but-connected points around the crossover) and the shape stamp (ADR-002) re-stamped — a normal iteration, not a gate failure. Downstream planning should budget for one such revision pass rather than treating the Wave-1 corpus as final.

**Reuse note:** depends on the fixture corpus loader and alias map from this feature's corpus submodule; depends on `find_terminal_active` (`graph.rs:547`) for redirect-to-head semantics — reused, not reimplemented.
