## uni-zero product review (advisory — human judgment required)

**Gate**: scope-review | **Stance**: Scope the right problem, right-sized at the outcome altitude. Take P3 IN-SCOPE (OQ-1) — it is the last mile of the same isolation outcome, not a different one; deferring it ships the exact half-isolation this feature exists to kill.

### Vision / roadmap fit
Direct service to goal #5519 (personal-cloud). The scope targets the invariant #5519 names as **OSS-in-scope, not an enterprise deferral**: "one cloud serves N projects, each fully isolated … no cross-project sharing in OSS," realized through the `resolve_store(request)` single funnel so "cross-project contamination is structurally impossible." The scope correctly reads that this invariant is **currently not met** on the cloud transcript/knowledge/config paths — vnc-038 isolated only the store *write* funnel and left registry / pending / services / config-snapshots global. This closes that gap at the seam. Good roadmap hygiene: it refuses the two-line #930 patch that would ship a working transcript fold beside briefings still reading the wrong project's store.

Capability landscape it moves (multi-capability feature):
- **C0 #5594** (rollup / **curve**, partial) — the #930 transcript-fold fidelity is the D2/D3 parity behavior for the **multi-project** cloud case, which the current C0 matrix (nan-022 #837) never exercised — that matrix is single-slug. So this **advances C0 toward** the north-star on a deployment shape the proof doesn't yet cover; it does not "complete" a curve.
- **N3 #5593** (nfr, proven) — N3's proof is scoped to "the served **write** surface (observe + MCP-write) lands in A's **store**." The transcript→distillation→persisted-knowledge path (INV-T2 collision case) and the observe-path knowledge **read** funnel (P2) are surfaces N3's proof never touched. This feature **extends the isolation proof surface** — consistent with N3's own "Re-prove as the served write surface grows" clause. Not a flip; a proof-surface extension for the vision session to note.
- **C6 #5579** (functional, partial) — P3 is exactly per-slug config. C6's resolution logic is proven in-crate (vnc-040) but the scope reveals `build_project_server` **never wires** the resolved config into the per-slug observe path (silent fallback to builtin defaults). C6 is effectively *more* partial than the map reads. P3 in-scope advances C6 toward proven; its behavioral proof overlaps **#800** (infra-001 multi-slug HTTP fixture, OPEN) — coordinate so config-parity isn't proven twice.

Sequencing: #930 is the open entry-point blocker; no conflict with other in-flight personal-cloud work (#865 D5 parity, #829 UI research, #794 release harden are orthogonal).

### Approach commentary
Sound. Strengths: (1) fixes once at the funnel per governing pattern #5629 rather than patching 9 instances; (2) NG-6 correctly leaves the mechanism to the architect and fixes only the invariants; (3) AC-06 pins the primary gate at **outcome altitude** — behavioral proof through the public `/v1/{slug}/…` interface, no `Arc::ptr_eq` in that suite; (4) AC-08 boot assertion converts the whole "constructor-default never overwritten" bug **class** from silent-read-zero to loud-at-boot — a structural regression guard, not a point fix. The N≥2 constraint (pattern #5172, #4974) is correct: N=1 cannot distinguish a real funnel from a global-handle bypass.

Watch item — the `transcript_hold` + `session_registry` pairing (Constraints, design-reviewer F1): the scope flags that wiring the registry without the paired hold splits the purge gate into unbounded buffer growth. Good that it is a stated constraint; the design session must honor it as a unit.

### Capability coverage
Target: multi-capability — anchor is goal #5519's OSS per-project isolation invariant, decomposed across C0 #5594 / N3 #5593 / C6 #5579 (+ C4 #5533 one-seam discipline).
Archetype: **mixed** — C0 is a **curve** (rollup, "never terminal"); N3 and C6 are **threshold** (proven/partial floors).
Coverage: curve (C0) → **advances-toward** (multi-project D2/D3 fidelity the single-slug matrix omits); threshold (C6) → **advances toward proven** IF P3 in-scope, else untouched; threshold (N3) → **extends proof surface** to transcript/knowledge-read.
  NOTE: C0 is a curve — do NOT read the #930 fix as "completing" C0; it advances it. C6 is a threshold floor — its silent config-fallback is a real partial, and P3-in-scope is one build_project_server pass from closing it.
done_when clause map (against #5519 isolation invariant + affected capabilities):
 - Transcript fidelity own-read (INV-T1 / #930) → covered by AC-01 (advances C0 D2/D3 multi-project)
 - Transcript isolation, collision case (INV-T2) → covered by AC-02 (extends N3-class to transcript surface)
 - Pending-entries isolation (INV-T3) → covered by AC-03
 - Knowledge-read fidelity + isolation (INV-K1/K2, P2) → covered by AC-04 (closes vnc-038 read-side gap)
 - Config fidelity + isolation (INV-C1/C2, P3) → covered by AC-05 **ONLY IF OQ-1 lands P3 in-scope**; DEFERRED (named gap) if cut — C6 stays partial, and the config half of Goal 2 stays a *silent* fallback unless AC-08's boot assertion still fires on those fields
 - Behavioral-suite-through-public-interface (Goal 3) → covered by AC-06
 - Loud-at-boot regression guard (Goal 4) → covered by AC-08
 - Vestigial-field deletion → covered by AC-09; ADR → AC-10
Verdict: **meets the defined part**, with one visibly-declared conditional gap (P3/config, OQ-1). No UNDECLARED gap — the scope names its boundary explicitly and its NG list (per-user, multi-tenant, cross-project sharing) is honest different-outcome enterprise defer.
Capability-status implication (recommendation only, vision session owns any tag flip): C0 stays ⚪ partial (advances the multi-project fidelity, does not flip the curve); N3 — recommend the vision session record that this feature **broadened N3's proof surface** beyond the store-write path (do not re-flip; annotate); C6 flips toward proven **only if P3 in-scope AND #800 exercises it end-to-end** — otherwise C6 stays partial.

### Right-sizing
Outcome / DoD altitude: the property must WORK and be ENFORCED — "post/read as A, B can NEVER observe A's transcript, knowledge, or config; A's own reads DO see A's data," proven behaviorally through the public HTTPS interface, plus loud-at-boot regression. The scope aims here, not at "the field exists." Correct altitude.
Size verdict: **right-sized at P1+P2+P3; TOO SMALL if cut to P1 only** (mechanism without outcome — a working transcript fold beside a live cross-project knowledge-read leak). The floor is P1+P2; P3 is the last mile.
Deferrals — for each:
 - NG-1 per-user/OAuth-subject isolation → DIFFERENT OUTCOME (enterprise per-user boundary) — legit defer.
 - NG-2 multi-tenant → DIFFERENT OUTCOME (enterprise) — legit defer.
 - NG-7 cross-project knowledge sharing / owner fan-out → DIFFERENT OUTCOME (enterprise) — legit defer.
 - **P3 config family (OQ-1)** → **LAST MILE of THIS outcome** (per-slug observe isolation), incremental effort **small** (same build_project_server pass, mirror main.rs:978-989, 3 already-available params) → **pull in now.**
Follow-up smell: **Yes.** OQ-5 pre-negotiates filing a P3 follow-up issue. The thing being teed up as a "tracked follow-up" is the config half of Goal 2 ("cross-project contamination structurally impossible on the transcript, knowledge, **and config** paths") — i.e. part of the stated point of this work. When the deferred item is named in the goals and costs one incremental pass, that is the signal to pull it in, not file it out.

### Recommended answers to open questions
1. **P3 scope boundary — in-scope now, or tracked follow-up?** → **IN-SCOPE now.** Same seam, small incremental cost, and it closes the config half of the very outcome Goal 2 names. Deferring ships transcript+knowledge isolation beside config that still silently reads builtin/global defaults — the inconsistent half-isolation the problem statement itself rejects. Concrete refinement (handles OQ-3): scope the **behavioral** suite to the config fields that have a clean public surface (`signal_class_names` → `signal_class_counts`; observation categories → status; retention → purge behavior); for any field lacking one, rely on the **AC-08 boot assertion + a wiring-pin unit guard** — the boot assertion already covers the whole field class structurally, so a missing behavioral surface is NOT a reason to defer the field.
2. **P2 non-negotiable in-scope — confirm?** → **Confirm, no objection.** P2 is a cross-project knowledge-**read** leak with a privacy dimension — a slug reads another project's persisted knowledge store. That is a security-class defect, not an enhancement. Floor.
3. **Config fields lacking a behavioral surface — white-box exception or defer?** → **Accept a documented white-box guard** (AC-08 boot assertion + wiring-pin), recorded as an explicit AC-06 exception — **do not defer the field.** Deferring recreates the silent-fallback split-brain this feature exists to eliminate; the boot assertion still enforces the field is wired to the instance its write path uses. Letting "no public observation point" push a field to defer inverts the goal.
4. **White-box guards as required complements, not substitutes — confirm?** → **Confirm, required.** AC-08's boot assertion forecloses the whole bug class loud-at-boot (structural — catches unwired fields the behavioral enumeration might miss); the N≥2 behavioral suite proves the observable property implementation-agnostically. Complementary per pattern #5629; neither substitutes for the other.
5. **Human owns filing any deferred follow-up?** → **Confirm — human owns the filing** (standing project norm: don't auto-file outward commitments). Note: if OQ-1 lands P3 in-scope as recommended, there is **no** follow-up to file; this question only lives if a config field is ultimately cut.

### Recommended actions
1. Resolve OQ-1 **P3 in-scope** and OQ-2 **P2 confirmed** before the design session — both are the isolation outcome, not additive.
2. Instruct the design session to honor the `transcript_hold`+`session_registry` pairing as a unit (Constraints / design-reviewer F1) and the one-funnel discipline (methods on `Arc<dyn StoreResolver>`, no side-map — vnc-034 #4974 guard).
3. Coordinate the INV-C1/C2 behavioral proof with **#800** (infra-001 multi-slug HTTP fixture, OPEN) so config-parity end-to-end is proven once, not twice — this is also C6's path to proven.
4. Flag for the **vision session** (not this gate): this feature broadens **N3**'s proof surface beyond the store-write path to the transcript/knowledge-read surfaces, and reveals **C6**'s resolved-but-unwired config gap — both are capability-map annotations the vision session owns; no reviewer flip.
5. If, and only if, speed forces a cut, hold P1+P2 as the floor and have the human file the P3 follow-up per OQ-5 — with a PR risk note that cloud config still reads global defaults until then.
