# AC-03 Independent Fixture — Provenance Header (crt-052, OQ-6 / R-20)

> **REVIEW GATE (OQ-6).** This header asserts the independence of the labeled
> recall corpus from the regex marker set it validates. Without genuine
> independence, a ≥0.90 recall score is self-fulfilling (R-20). A reviewer must
> confirm the mode below before accepting the recall number.

## Independence mode: `anchors-before-port`

The labeled corpus (`labeled_corpus.jsonl` + `labels.json`) was authored from
the **ass-070 hand-labeled ground-truth descriptions** (FINDINGS.md Q1: 43
items across 6 real Claude Code sessions, hand-labeled by family with byte
anchors and one-line descriptions). Those labels were produced during the
ass-070 research spike **before** crt-052's regex set existed — i.e. the
anchors predate the port.

Concretely:

1. The fixture blocks here are **paraphrased reconstructions** of the kinds of
   user/assistant prose the ass-070 labelers anchored (decision phrasing,
   rework narration, lesson statements, phase/gate transitions), written to
   the family meaning a human reader assigns — NOT copied from the
   `markers.rs` pattern list.
2. The `labels.json` family assignment for each block is the human-reader
   judgment of what family the block belongs to, decided by reading the prose,
   independent of which regex (if any) happens to fire.
3. The recall test (`test_independent_corpus_recall_ge_090`) then runs the
   committed regex set over these independently-labeled blocks and measures the
   fraction of labeled blocks the rules recover. ass-070 measured 0.93 block
   recall for the original 50-pattern set on the original corpus; this fixture
   targets the same ≥0.90 floor on independent prose.

## Author / order attestation

- **Author:** crt-052 C3 selection-module delivery agent.
- **Authoring order:** the family labels follow the ass-070 ground-truth
  semantics (authored in the research spike, predating this regex port). The
  blocks were written to those pre-existing label semantics, then the regex set
  was scored against them. The labels were NOT derived by reading the regex
  patterns.
- **Anti-circularity:** no block text was lifted verbatim from a `markers.rs`
  pattern; each block is natural protocol-narration prose. A future re-author
  who suspects circularity should rewrite the prose (keeping the human family
  label) and confirm recall stays ≥0.90 — that is the standing enforcement.

## Files

- `labeled_corpus.jsonl` — Claude Code JSONL transcript lines (the raw input).
- `labels.json` — per-line family labels + the `raw_bytes` total for the
  volume check.
- `malformed/` — the AC-V-FUZZ adversarial corpus (truncated JSON, non-UTF-8,
  oversized line, unknown record type, embedded NUL, deeply-nested JSON).
