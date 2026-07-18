# FINDINGS: Deterministic two-tier hotspot bank for `context_cycle_review`

**Spike**: ass-095 · **Date**: 2026-07-10 · **Approach**: measurement + proof-of-concept · **Confidence**: empirical
Tracking: GH #938 · Origin POC: dug-21/arch-research#38 (`smart-edge-002/003`, graded `partial`)

Ephemeral Rust harness on disk at `product/research/ass-095/harness/` (`Cargo.toml`, `src/main.rs`,
`failure_snippets.txt`; `target/` removed — rebuild with `cargo run --release`).

## Data used (all ours, all real)
- **Live observation store** (`~/.unimatrix/…/unimatrix.db`) — 55,387 `ObservationRecord` rows,
  126 sessions, **639 `PostToolUseFailure` events** with `response_snippet`, `sessions.feature_cycle`
  linkage. The exact surface the 22-rule engine runs on.
- **Lesson corpus** (`knowledge/unimatrix.jsonl`) — 2,590 `lesson-learned` (190 auto/engine-output
  + 2,400 narrative).
- **Engine baseline** = the persisted "Retrospective findings: {cycle}" nodes (#5067 crt-055,
  #5471 crt-058, #5391 vnc-042) — the current engine's own output (rule names `tool_failure_hotspot`,
  `cold_restart`, `mutation_spread`, …). Authoritative baseline.

**Reframing structural fact**: the 22-rule engine does **no** text-signature matching over response
text. `response_snippet` is read once (`ToolFailureRule`, `friction.rs:143`), only as evidence *after*
a count-based hit. Every rule is structural/statistical. Bank and engine occupy **near-disjoint surfaces.**

---

## G1 — Coverage delta (money question)
**Bank and engine are complementary, not competing.** The bank's genuine addition is a narrow band —
**semantic single-instance failure classes that never cross a count threshold**, chiefly the write-block
class the review missed. The engine owns the entire statistical/temporal/breadth tier, to which the bank
is blind. **The load-bearing hypothesis "the bank meaningfully beats the 22-rule engine" is FALSE as stated.**

- `ToolFailureRule` fires at **>3 same-tool failures/session**. **135 of 191 failure clusters — 217 of
  639 events — are sub-threshold** and invisible to it.
- **Write-block class** (`Agent 'anonymous' lacks Write capability`, MCP `-32003`; lesson #5465):
  **53 events across 23 sessions / 20+ cycles**. 33 sit in sub-threshold clusters → engine blind in
  ~18 of 23 sessions; where it fires it emits only `"Tool context_store failed N times"`, no semantic class.
- **Decisive**: across **all 185 retrospective (engine-output) KB nodes, 0 contain any semantic
  failure-class literal**. The engine has never surfaced this class in recorded history despite 53
  occurrences — which is why a human hand-authored #5465. The bank recovers it at one occurrence.
- Smaller adds: payload-too-large (`-32602`) 28, rate-limit 4, permission 1.
- **Engine-adds / bank-blind**: `session_timeout`, `cold_restart`, `file_breadth`, `reread_rate`,
  `mutation_spread`, `edit_bloat`, `adr_count`, `design_artifact_count`, `source_file_count`. crt-055's
  13 findings are 100% structural.
- **Precision hazard in the add**: a naive bank fires **246× on `file_not_found`** (routine `Read`
  misses) and matched 273/639 on no curated signature — the delta is real only with a curated set.

**Recommendation**: Build as a complementary semantic-failure tier for the curated band only; keep the
22-rule engine as backbone. Do not frame as replacing the engine. **Confidence: high.**

## G2 — Signal-source viability
**Yields a small curated bank today (~5–10 usable runtime-failure classes), not thousands.
"Sharpens as the KB grows" does not hold** — bottlenecked and partly circular.
- 2,590 lessons = 190 auto/engine-output (**circular** — sourcing them feeds rule-name strings back) +
  2,400 narrative. Only **26 of 2,400 (1.1%)** carry an extractable error literal, most code-level.
  Usable runtime classes: write-block, payload-too-large, rate-limit/overload, refusal stems,
  permission-denied.
- **Data hygiene**: auto-reading `lesson-learned` wholesale → ~2,400 irrelevant phrases → precision
  collapse; and the retro-output nodes are circular (engine output stored back as lessons). Use a
  curated catalog, not the category. (An earlier "self-starving feedback trap" interpretation here was
  retracted 2026-07-10 as backwards — the failure signal is behavioral and human-independent; see
  Out-of-Scope Discoveries.)

**Recommendation**: Source from a **curated, human-gated config catalog** (like `SignatureScanner`'s),
not an auto-read of the category. Don't advertise self-sharpening. **Confidence: high.**

## G3 — Precision on our real transcripts
**The fix holds decisively.** Re-expressed for our shape (no exit_code → anchor on
`event_type == "PostToolUseFailure"` + `response_snippet`), scoping moves precision **0.67 → 1.00 with
zero recall loss**.
- **Naive** (scan `response_snippet`+`input`, all hooks): 79 fires; **26 are discussing/authoring** →
  precision 53/79 = 0.67.
- **Scoped** (`PostToolUseFailure`→`response_snippet`): **53/53 → 1.00**, recall preserved. Reproduces
  the POC's 3/3→0/3, now quantified at scale (26 FPs eliminated).
- The missing exit_code field is a non-issue: `PostToolUseFailure` *is* our outcome field.

**Recommendation**: Anchor every match to `event_type == PostToolUseFailure` → `response_snippet` only.
Never scan `input` or prose. **Confidence: high.**

## G4 — Engine choice on our build
**Scaling asymmetry reproduces but is irrelevant at our signature count — reuse `RegexSet`; do NOT add
first-class `aho-corasick`.**

| N | AC build | AC scan | RegexSet build | RegexSet scan |
|---|---|---|---|---|
| 100 | 0.9 ms | 959 MB/s | 2.5 ms | 798 MB/s |
| 1,000 | 1.3 ms | 866 MB/s | 7.5 ms | 535 MB/s |
| 10,000 | 14.7 ms | 857 MB/s | **ERR CompiledTooBig** | — |
| 100,000 | 274 ms | 846 MB/s | skipped | — |

- aho-corasick flat ~850–960 MB/s to 100k (confirms POC). `RegexSet` hits the 10 MB compile ceiling at
  10k (reproduces POC) and is slower even at 1k.
- **But our bank is <20 signatures** (G2); `SignatureScanner` caps at `MAX_SIGNAL_CLASSES=16`. The real
  639-snippet corpus scans in **148 µs total either way**. aho-corasick's edge appears only at 10k+ —
  unreachable here.

**Recommendation**: Reuse `regex::RegexSet`. Revisit only above ~1,000 curated literals (not
foreseeable). **Confidence: high.**

## G5 — Go/no-go + design input
**Conditional GO on a scoped-down build; NO-GO on the #938 two-tier aho-corasick bank as written.**
#938's structural tier duplicates the 22 rules (G1), aho-corasick is unjustified (G4), and
lesson-sourced self-sharpening is bottlenecked/precision-risky (G2).

**Minimal build (recommended)**: one new `DetectionRule` — `SignatureFailureRule` in
`detection/friction.rs` — filtering to `PostToolUseFailure`, matching `response_snippet` against a
**small curated `RegexSet` (~10 literals)** from a config catalog (mirror `[transcript_signals]`),
emitting `Friction`/`Warning` with a semantic class label + snippet evidence. Precision 1.00 by
construction, reuses `RegexSet`, lives in the existing trait framework, recovers the write-block class.

**Ranked options**:
1. **(c) New `DetectionRule` + curated `RegexSet` scoped to `PostToolUseFailure` — RECOMMENDED.**
2. **(b) Extend `SignatureScanner` — REJECTED**: wrong surface (live raw transcript bytes over all prose
   = the unscoped precision-0.67 config); keep it for its current job.
3. **(a) Lesson-sourced aho-corasick two-tier bank (#938) — REJECTED**: duplicative + unjustified + circular.
4. **(d) No-go — REJECTED**: a real recurring missed class exists (53 events / 23 sessions).

**Recommendation**: GO on (c). Open a design session for a single `SignatureFailureRule` + curated
human-gated failure-signature catalog; de-scope aho-corasick, the structural tier, and auto-sourcing.
**Confidence: high for ranking; directional for exact build shape.**

---

## Unanswered Questions
- **Cross-source failure surfaces** (`source_domain != "claude-code"`) — out of scope; all measurement
  used `claude-code`.
- **Rule severity/threshold** (fire on 1 event? require recurrence across ≥N sessions to suppress
  one-offs like the 247 `file_not_found`?) — design decision, not measurable here.
- **Label taxonomy / catalog ownership** (config vs curated Unimatrix list) — design input.

## Out-of-Scope Discoveries
- **`SignatureScanner` runs the naive unscoped approach in production today** — counts error/refusal over
  all live transcript bytes (the precision-0.67 config); its counts likely include discussing-vs-event
  contamination. Worth a separate review.
- **Retrospective findings are stored back as `lesson-learned` (190 nodes)** — polluting the category with
  circular engine output; any future feature reading `lesson-learned` for signal must filter
  `source:retrospective`.
- **`PhaseDurationOutlierRule` is structurally inert** (`scope.rs` returns `vec![]`; detection delegated
  to `baseline.rs`) — occupies a rule slot, emits nothing in-engine.
- **RETRACTED (2026-07-10) — the "feedback trap / self-starving loop" interpretation was backwards.**
  An error trending to zero is elimination of the bad issue (the goal), not a lost signal; and novel-class
  discovery is behavioral, not lesson-dependent — 273/639 failures here matched no curated signature, which
  IS the unnamed-class beacon and needs no human authorship. Automating known-class detection frees human
  attention toward that residual. The sound, surviving point is data hygiene only: don't auto-source from
  `lesson-learned` (1.1% literal density + circular) — curate the catalog.

## Recommendations Summary
- **G1**: Bank complements, does not beat, the engine — narrow sub-threshold semantic-failure band only;
  hypothesis false as stated.
- **G2**: ~5–10 usable signatures today, not thousands; self-sharpening fails; use a curated human-gated
  catalog, not auto-read of `lesson-learned`.
- **G3**: Anchor to `PostToolUseFailure`→`response_snippet`; precision 0.67→1.00, recall preserved;
  exit-code gap is a non-issue.
- **G4**: aho-corasick scaling win reproduces but is irrelevant at <20 sigs — reuse `RegexSet`, no
  first-class aho-corasick dep.
- **G5**: GO on one new `SignatureFailureRule` (curated `RegexSet` scoped to `PostToolUseFailure`);
  NO-GO on the #938 two-tier aho-corasick bank.
