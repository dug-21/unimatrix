# ADR-005 (nan-018): Two-Corpus Model, Recommendation-Only Band-3 Boundary, and Wave Independence

### Context

nan-018 is a **wide** feature (config exposure + trust metric class + cost metric + fixture corpus + drift guard + three doc bands + a protocol recommendation). Three decisions must be locked so delivery does not collapse them into an all-or-nothing monolith or violate explicit non-goals:

1. **Two-corpus durability model** (AC-07) — a primary durable corpus vs an ephemeral realism snapshot.
2. **Band-3 boundary** (AC-12/13, SR-04) — nan-018 must edit **no** `.claude/protocols/` file; the protocol trigger is a recommendation handed off for separate ratification (OQ-5, human-later). C-07 is DISSOLVED (no protocol-vs-corpus sequencing constraint, since there are no protocol edits).
3. **Wave independence** (SR-05) — Wave-1 (instrument core, AC-01..09 + AC-14) must ship without Wave-2 (docs + Band-3) coupling, and AC-14 (proof-by-use sweep) is the Wave-1 exit.

### Decision

**1. Two-corpus model (AC-07).**
- **Primary = fixture corpus** (durable yardstick): hand-authored entry-graphs (ADR-004), property/alias assertions, carries the ADR-002 shape stamp. The trust/correctness spine. Lives in-repo, version-controlled, under `crates/unimatrix-server/src/eval/corpus/fixtures/`.
- **Realism layer = production snapshot** (ephemeral): the existing `snapshot` path; supplies realistic P@5/MRR baselines; re-snapshot when shape drifts (the shape guard warns rather than errors on the snapshot).
- Both flow through the **same** `EvalServiceLayer::from_profile`. The distinction is the *assertion style* and the *durability contract*, **documented, not enforced by a code branch** — keeping replay machinery single-path and reused (cumulative test infra). Band-2 documents which is which and when to re-snapshot.

**2. Band-3 = recommendation only; zero protocol-file edits (AC-12a/13, SR-04).**
- nan-018 ships **one document**, `product/features/nan-018/RECOMMENDATION-band3-protocol.md`, recommending a `[CONDITIONAL]` protocol step patterned on the existing `[CONDITIONAL] uni-docs` step.
- The recommended predicate (OQ-5, human-ratified): the conditional fires when **"your change alters the retrieval-shape hash"** — coupled to the ADR-002 hash, **not** an enumerated list. This is deterministic (the hash moves or it doesn't — no delivery-leader judgment), precise (only shape-affecting changes fire), and unified (the mechanical guard and the protocol trigger are ONE definition of "shape"). The enumerated input set in ADR-002 becomes *documentation of what feeds the hash*, not the trigger itself.
- The recommendation **describes** how the design and delivery/bugfix protocols *would* carry a conditional eval-corpus-migration step (asset-maintenance only: keep the corpus valid + validate it loads — explicitly NOT eval-execution-as-quality-gate, which is a separate future design and a non-goal).
- **nan-018 edits no `.claude/protocols/` file.** A later uni-zero session ratifies and applies it. Because nan-018 makes no protocol edits, there is no forward reference to a not-yet-existing corpus and no sequencing constraint (C-07 dissolved).
- Layers that DO ship inside nan-018: the mechanical guard (ADR-002, code) and the Unimatrix knowledge — a `convention` ("schema/shape change ⇒ corpus migration", surfacable in briefing) and `procedure` entries (how to migrate, how to author a scenario), stored at retro.

**3. Wave independence + AC-14 exit (SR-05).**
- **Wave-1 (load-bearing spine, AC-01..09):** penalty config (ADR-001), trust class (ADR-004), cost metric (ADR-003), fixture/primary corpus + property assertions (ADR-004), two-corpus model plumbing, drift guard (ADR-002). Exit gate = **AC-14**: a single correlated sweep on the fixture corpus where an exposed steepness lever moves and trust outcomes + P@5/MRR + cost are all reported in one run. This is what unlocks the rewritten ass-073.
- **Wave-2 (deferrable, AC-10..13):** Band-1/2 docs, the Unimatrix `convention`/`procedure`, and the Band-3 recommendation doc. These have **zero code coupling** to the instrument core — they reference behavior and the hash *conceptually*, import no Wave-1 type. They may land later without blocking measurement.
- Boundary discipline (SR-04, hard gate items): **no `.claude/protocols/` edits at all**; **cost is built explicitly, never narrowed-as-deferral** (ADR-003).

### Consequences

**Easier:** Wave-1 can ship and unblock the downstream spike chain (nan-018 → rewritten ass-073 → ass-074 → crt-053) before docs are complete. The single-path corpus plumbing means the durable fixture and the ephemeral snapshot share all replay/metric code — no fork to maintain. The recommendation-only Band-3 keeps nan-018 inside its boundary (AC-13) while still delivering the live mechanical guard that actually protects the corpus.

**Harder:** The two-corpus distinction being doc-enforced (not type-enforced) means a future contributor could author literal-ID `expected` into the primary corpus; mitigated by the ADR-004 corpus-validation ban (load-time rejection). The Band-3 recommendation is inert until a separate uni-zero session acts on it — the corpus is protected in the interim only by the mechanical guard, which is by design the durable backstop. Delivery must resist the SR-04 pressure to (a) edit a protocol file "while we're here" or (b) narrow cost into a deferral — both are explicit FAIL conditions, not judgment calls.
