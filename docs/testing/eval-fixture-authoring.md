# Fixture-Corpus Authoring Guide

Band-2 guide (nan-018, AC-11). How to author a fixture entry-graph with
property-based assertions, sufficient to add a scenario **from this page alone**.
Pairs with:

- [Two-corpus model](./eval-two-corpus-model.md) — why the fixture corpus is the durable primary
- [Schema-migration runbook](./eval-corpus-migration.md) — what to do when the drift guard trips
- [Config-knob reference](./eval-config-knobs.md) — the penalty levers a sweep tunes
- [Evaluation harness overview](./eval-harness.md)

The fixture corpus lives at
`crates/unimatrix-server/src/eval/corpus/fixtures/` — one `*.toml` file per status
shape, plus a `manifest.toml` stamp. It is **version-controlled, hand-authored,
and small by design**.

---

## The non-negotiable rules

1. **Property assertions ONLY — never literal IDs.** The primary corpus asserts
   *relationships* (`redirect_to_head`, `forbidden_absent`, `rank_below`), never
   `expected = [42, 17]`. The loader **rejects** a literal-id `expected`
   (`CorpusError::LiteralIdExpected`) and rejects a scenario with no ground truth
   at all (`CorpusError::NullExpected`). This is crt-013 #703 — "assert outcomes,
   never constants" — and it is what keeps assertions valid across re-snapshots.
2. **Alias discipline — author against stable aliases, never raw ids.** Every
   entry carries a stable `alias` (e.g. `"jwt.head"`); ids are assigned at load
   and are never written by hand. Assertions reference aliases; the loader resolves
   them. Re-snapshotting renumbers ids but aliases are stable, so the assertion
   survives. A reference to an undefined alias is a **hard load error**
   (`CorpusError::MissingAlias` / `DuplicateAlias`) — never a silent vacuous pass.
3. **The corpus is NOT frozen.** Budget one revision pass: the downstream
   measurement spike (ass-073) may find the Wave-1 corpus needs more bracketing
   points around the steepness crossover. "Spike finds corpus insufficient → revise
   + re-stamp" is an *anticipated, valid loop*, not a failure (ADR-004 §5).

---

## On-disk format

Each fixture file is TOML with `[[entries]]` and `[[scenarios]]` arrays. Parsed by
`eval/corpus/assertions.rs` (`RawFixture` / `RawEntry` / `RawScenario`).

```toml
[[entries]]
alias = "jwt.head"                 # stable handle, globally unique across the corpus
title = "JWT validation current guidance"   # searchable text
content = "validate exp nbf aud iss claims pin the signing algorithm ..."  # searchable text
status = "Active"                  # "Active" | "Deprecated" (others default to Active)
superseded_by = []                 # ALIASES of entries that supersede THIS one — resolved at load
category = "auth"

[[scenarios]]
id = "multi-correction-chain.redirect"   # optional; defaults to "corpus-{index}"
query = "how should I validate a JWT token"
[scenarios.assertions]
redirect_to_head = ["jwt.head"]
forbidden_absent = ["jwt.stale"]
rank_below       = [["jwt.v1", "jwt.head"]]   # [A, B] pairs: A must rank strictly below B
```

### `superseded_by` semantics (read carefully)

`entries[X].superseded_by = ["Y"]` means **alias `Y` supersedes alias `X`**,
producing a Supersedes edge `X -> Y` (old -> new). The loader wires both columns:
`X.superseded_by = Y.id` and `Y.supersedes = X.id`. The graph builder derives the
Supersedes edge from the `supersedes` column — no `graph_edges` rows are authored.

### Fields you do NOT author

Ids, embeddings, hashes, timestamps, confidence — all assigned/zeroed at load.
`status` parses only `Active` / `Deprecated` / `Proposed` / `Quarantined`; anything
else defaults to `Active`. Keep authored statuses to `Active` / `Deprecated`.

---

## The three property assertion types

Evaluated in `eval/runner/trust.rs::evaluate_trust` against the ranked result list
(index 0 = best rank). Aliases resolve to ids at load.

### `redirect_to_head` — the chain head must surface at/above stale members

For a query targeting a corrected chain, the **terminal Active head** (resolved by
the engine's `find_terminal_active` semantics) must appear in results, and **no
present superseded member may outrank it**.

- **Pass:** head present AND every present superseded member ranks at-or-below it.
- **Fail:** head absent; OR a superseded member outranks the head.

### `forbidden_absent` (absence) — the stale source must not appear

- **Pass:** `forbidden ∩ top_k == ∅`.
- **Fail:** any forbidden alias present in results.

### `rank_below` `[A, B]` — A must rank strictly below B

This is the **asymmetric** one. Get it wrong and you invert the metric.

| Case | Verdict |
|------|---------|
| A present, B present, `rank(A) > rank(B)` | **PASS** |
| A present, B present, `rank(A) <= rank(B)` | **FAIL** |
| **A absent** | **PASS** (absent is "below everything") |
| **B absent while A present** | **FAIL** (the should-rank-higher anchor is missing) |
| Both absent | PASS (A-absent arm dominates) |

> **The single most likely authoring bug:** assuming "either absent ⇒ pass." It is
> **not** symmetric. `A absent ⇒ PASS`, but `B absent (while A present) ⇒ FAIL`.
> When you assert `["deprecated.x", "active.weakest"]`, you are asserting both that
> the deprecated entry stays low *and* that the weakest active stays present.

An unresolvable alias (a load-invariant breach) is surfaced as a distinct
violation and **fails** the relevant verdict — never treated as "absent" (never a
vacuous pass).

---

## The five status shapes (author each as its own file)

AC-06 requires at minimum the first four. The shipped corpus authors all five.

| File | Shape | Penalty branch exercised | Assertions used |
|------|-------|--------------------------|------------------|
| `multi_correction_chain.toml` | A→B→C→head (depth > 1, single Active terminal) | `clean_replacement × hop_decay^(d-1)` + clamp | redirect-to-head + rank-below |
| `dangling_deprecated.toml` | Deprecated entry, no successors | orphan | absence + rank-below |
| `superseded_active.toml` | Entry marked superseded yet still `Active` | status/topology conflict | redirect-to-head + rank-below |
| `deprecated_connected.toml` | Deprecated but reachable via positive edges | absence / rank-below under connectivity | absence + rank-below band |
| `dead_end_chain.toml` (optional 5th) | Superseded chain reaching no Active terminal | dead_end | redirect (a **defined fail** edge) + rank-below |

Always include at least one **Active** entry per file so the result set is
non-empty and assertions evaluate against real results, not vacuously (R-15).

---

## Authoring DEPTH obligation (ADR-004 §5 — beyond the AC-14 floor)

AC-14 only proves the corpus *measures something*. It does **not** prove the corpus
is a good-enough yardstick for the downstream steepness question. The corpus must
be authored **beyond that floor**, and the load-bearing shape is
**deprecated-but-connected**.

The requirement: the steepness crossover — the (similarity, connection-strength)
point at which a connected-deprecated entry crosses the weakest-active threshold —
must sit inside a **bracketed range** of points, **not a single exemplar**. A sweep
over the penalty levers must cross the threshold at a *sequence* of distinct
points.

Concretely, `deprecated_connected.toml` authors:

- a **spread of connected-deprecated entries** spanning query-similarity (verbatim
  query terms → few shared terms) AND connectivity (standalone-deprecated, depth-1
  chain, depth-2 chain) — the shipped corpus uses five (`db.dep1..db.dep5`); and
- a **band of Active entries** (`db.actStrong` / `db.actMid` / `db.actWeak`) so
  "the weakest active" is itself a spread, not a single line.

The `rank_below` assertions then pair deprecated entries against *different* active
anchors across the band — so a penalty sweep crosses the threshold at a sequence of
points, yielding the bracket §5 demands. **When you extend this shape, preserve the
spread** — collapsing it to one deprecated/one active pair re-creates the
single-exemplar weakness.

---

## Loading and running

The loader materializes the corpus into a snapshot DB the existing
`EvalServiceLayer::from_profile` consumes unchanged (the corpus is just another
snapshot source):

- `load_fixture_corpus(dir, target_db)` — parse + validate + materialize; returns a
  `LoadedCorpus { db_path, alias_map, scenarios }`.
- `load_fixture_corpus_with_embeddings(dir, target_db, provider)` — same, plus
  embed-at-load so the snapshot is end-to-end **searchable** (ADR-002 branch (b));
  this is what makes trust assertions non-vacuous.

The end-to-end correlated sweep over the fixture corpus is `run_fixture_sweep`
(`eval/runner/sweep.rs`) — the AC-14 proof-by-use harness. It is exercised by the
`sweep_tests.rs` proof test; see [the two-corpus model](./eval-two-corpus-model.md)
for how the fixture path relates to the CLI `eval run` snapshot path.

### Validation the loader enforces (fail-loud, never silent)

| Condition | Error |
|-----------|-------|
| literal-id `expected` in a primary scenario | `CorpusError::LiteralIdExpected` |
| scenario with no assertions and no `expected` | `CorpusError::NullExpected` |
| same alias defined twice | `CorpusError::DuplicateAlias` |
| assertion / `superseded_by` references an undefined alias | `CorpusError::MissingAlias` |
| author-supplied path escapes the corpus dir | `CorpusError::PathTraversal` |

After authoring a new scenario, re-stamp the corpus **only if** the change altered
the shape inputs — usually it does not (adding entries within the existing shape
leaves the retrieval-shape hash unchanged). See the
[migration runbook](./eval-corpus-migration.md).
