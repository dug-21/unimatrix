# SCOPE — ass-095: Deterministic two-tier hotspot bank for `context_cycle_review`

Tracking: GH #938 · Origin POC: dug-21/arch-research#38 (`smart-edge-002/003`, graded `partial`).

## Framing

`context_cycle_review`'s hotspot detection today (`crates/unimatrix-observe/src/detection/`)
is a **22-rule `DetectionRule` engine** — deterministic, hardcoded statistical/counting
heuristics over persisted `ObservationRecord` events (`tool` / `event_type` / `input` /
`response_size` / `response_snippet`). It is already *structural*; it does **no** text-signature
matching, no config-driven signatures, no lesson-learned sourcing.

Signature matching over text **already exists** in adjacent subsystems via `regex::RegexSet`:
`distill/markers.rs` (~50 static candidate-selection patterns) and
`server/src/infra/transcript_activity.rs` `SignatureScanner` (config-sourced `bytes::RegexSet`,
error/refusal classes). `aho-corasick` is compiled in transitively (via `regex`) but is **not**
a first-class dependency.

#938 proposes a two-tier in-server bank: a **lexical tier** (`aho-corasick` literal signature
bank sourced from lesson-learned nodes, self-sharpening as the KB grows) + a **structural tier**
(match on the event structure, not raw prose). The structural tier substantially **overlaps the
existing engine**. The genuine novelty is the lesson-sourced lexical bank and the precision fix
(scope matches to the event outcome, not agent-authored content).

The decision this spike enables: **before investing in technology that overlaps what we already
run, prove empirically whether it catches real hotspots the current 22-rule engine misses — at
acceptable precision on our real data.**

## Goal (answerable questions)

- **G1 — Coverage delta (the money question).** On real Unimatrix-linked cycle data with
  documented hotspots (including the shd-005 subagent-write-block class the review missed), how
  many hotspots does the proposed two-tier bank surface that the current 22-rule engine does
  **not**, and vice versa? Quantify the union and each side's blind spots. Bank-vs-current-engine,
  not bank-vs-nothing.
- **G2 — Signal-source viability.** Can our *actual* `lesson-learned` corpus (`query_by_category`,
  currently written by review but never read back) yield a useful literal signature bank today?
  How many usable signatures does it produce now, and does the "sharpens as the KB grows"
  mechanism hold under measurement?
- **G3 — Precision on our real transcripts.** The POC's precision fix anchors matches on an event
  `outcome`/`exit-code` field. **Our events have no structured exit_code/outcome field** — failure
  is `event_type == "PostToolUseFailure"` + free-text `response_snippet`. Does the structural-scoping
  fix hold on our actual event shape? Measure recall and the discussing-vs-event false-positive rate
  on a real linked buffer.
- **G4 — Engine choice on our build.** Do the POC's scaling numbers reproduce in our workspace
  (aho-corasick flat ~285 MB/s to 100k; RegexSet collapse + 10 MB compile ceiling)? Does adopting
  `aho-corasick` as a first-class dep beat **reusing the `RegexSet` infra we already run**?
- **G5 — Go/no-go + design input.** Given G1–G4: worth building in-server? Where does it slot —
  a new `DetectionRule`, a new scanner tier, or an extension of the existing `SignatureScanner`?
  What is the minimal integration?

## Breadth
`code+ecosystem` — code-dominant. Internal: current engine, event/transcript surfaces, lesson
corpus, existing RegexSet infra. Ecosystem: aho-corasick vs RegexSet (largely settled by the POC;
reproduce, don't re-derive from scratch).

## Approach
`measurement` + `proof-of-concept`. Build an **ephemeral** measurement harness under
`product/research/ass-095/` (not committed product code, no PR) pointed at **our** data and
comparing against **our** 22-rule baseline. Mirrors how the smart-edge POC was built, but the
baseline is our current engine and the data is ours.

## Confidence required
`empirical` — data from measurement on our own system/corpus. A directional recommendation is
insufficient: the point is to know whether it *truly helps* before investing in overlapping tech.

## Target outputs
- Go/no-go decision on an in-server two-tier bank.
- Coverage delta, precision, and scaling **data + interpretation** on our data.
- Ranked options: (a) lesson-sourced aho-corasick bank, (b) extend existing `RegexSet`
  `SignatureScanner`, (c) add a structural `DetectionRule` for the missed class, (d) no-go.
- Design input: integration point + minimal-build shape if go.

## Constraints

**Hard (fixed):**
- Pure-Rust, in-binary, no FFI.
- Signals run over server-side `ObservationRecord` / transcript surfaces as they exist.
- Existing `DetectionRule` trait framework is the integration substrate.
- **No structured exit_code/outcome field** — failure = `PostToolUseFailure` event_type +
  free-text `response_snippet`. The POC's "scope to outcome field" fix must be re-expressed
  against this shape.
- Research produces **no committed product code, no PR, no Unimatrix writes**. Harness is ephemeral.

**Hypothesis (challengeable — treat as positions to test, not givens):**
- The bank belongs in-server.
- `aho-corasick` over `RegexSet` for the literal bank (two-engine split: regex for complex
  signatures, aho-corasick for the literal bank).
- Signatures sourced from lesson-learned nodes self-sharpen usefully.
- Structural scoping to the outcome field fixes transcript precision.
- **The bank meaningfully beats the current 22-rule engine** — the load-bearing hypothesis.

## Dependencies
None upstream. If `go`, unblocks a design session for in-server integration.

## Prior art
- Current engine: `crates/unimatrix-observe/src/detection/mod.rs` (22 rules, 4 categories);
  `dependency_on_deprecated` retired (#891).
- Existing signature infra: `distill/markers.rs` (`RegexSet`, ~50 patterns);
  `server/src/infra/transcript_activity.rs` `SignatureScanner` (config-sourced `bytes::RegexSet`).
- Lesson-learned storage: `query_by_category("lesson-learned")`; `write_lesson_learned`
  (`server/src/mcp/tools.rs`) currently writes, never reads for signatures.
- Event surface: `ObservationRecord` (`crates/unimatrix-core/src/observation.rs`); event stream
  loaders in `crates/unimatrix-store/src/observations.rs`.
- **POC evidence (smart-edge-002/003, graded `partial` — empirical mechanism, not in-server proven):**
  - Recall 11/11 (100%), 0 FP on a labeled real-event set; recovered the write-block hotspot the
    review missed (union of tool + manual-retro baselines).
  - Real corpus scan: 5 subagent transcripts, 919 KB, 199 ms, ~5.4 MB RSS.
  - Scaling (first-party): RegexSet collapses (10k = 48 s, hits 10 MB compile ceiling);
    aho-corasick flat ~285 MB/s at 100k signatures, 44 ms compile.
  - Precision failure on a real Unimatrix-linked transcript: naive lexical bank fired `firewall`/
    `error` on agents *discussing* hotspots (e.g. H29 node "…never a bare context_tag").
  - Structural fix demonstrated: scoping to `tool + response/outcome` only dropped false positives
    3/3 → 0/3 with recall preserved 5/5.
  - Honest gap to `proven`: never built in-server; never compared against our current engine.
    **G1 and G3 close exactly that gap on our data.**
