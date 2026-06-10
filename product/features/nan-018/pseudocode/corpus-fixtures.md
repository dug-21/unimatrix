# Component: Primary fixture corpus assets — `corpus-fixtures.md`

**Wave**: 1
**Location**: `crates/unimatrix-server/src/eval/corpus/fixtures/` (new TOML assets + manifest stamp).
**ADR**: ADR-004 (#4898 §5 authoring depth), ADR-002 (#4895 stamp). **Risks**: R-09, R-15 (non-degenerate).

## Purpose

The hand-authored, in-repo, version-controlled durable yardstick. Covers (at minimum) the four
required status shapes, each exercising a distinct `graph_penalty` branch, with property-based
assertions ONLY. Authored COLD (ass-073's feasibility probe was dropped) so it must EXCEED the
AC-14 floor — enough variation that the steepness crossover is findable in a *bracketed range*,
not a single exemplar (ADR-004 §5). NOT frozen: one revision pass budgeted.

## Required shapes (≥ four; optional 5th)

| Shape | Topology | `graph_penalty` branch exercised | Property assertions |
|-------|----------|----------------------------------|---------------------|
| **Multi-correction chain** | A→B→C→head (depth>1, single Active terminal) | `clean_replacement × hop_decay^(d-1)` + clamp; redirect-to-head | `redirect_to_head=[head]`; `rank_below=[(A,head),(B,head)]` |
| **Dangling deprecated** | Deprecated, no successors | `orphan` | `forbidden_absent=[dangling]` (stale must not surface) |
| **Superseded-but-Active** | Marked superseded yet `Active` | status/topology conflict path | `rank_below=[(superseded, active_head)]` |
| **Deprecated-but-connected** | Deprecated, reachable via positive (non-Supersedes) edges | absence / rank-below under connectivity | `forbidden_absent=[deprecated]` and/or `rank_below=[(deprecated, weakest_active)]` |
| **Dead-end chain** (optional 5th) | Superseded chain reaching no Active terminal | `dead_end` | redirect-to-head with NO valid head ⇒ defined FAIL (used to test the edge, not as a passing scenario) |

## Authoring depth (ADR-004 §5 — beyond the AC-14 floor)

- The **deprecated-but-connected** shape carries the MOST variation: author multiple connected-deprecated
  entries at a spread of similarity/confidence levels so the steepness crossover (the sim/conf point
  where a connected-deprecated entry crosses the weakest-active threshold) sits inside a BRACKETED
  RANGE of points. ass-073's sweep must rest on real evidence, not one exemplar.
- Author enough chain depth (≥3 in the multi-correction chain) that `hop_decay` and the clamp are
  both exercised at d=1 and d>=2.
- The corpus is small + curated by design (Non-Goal 8); large enough to exercise each trust shape
  end-to-end (FR-19) and to make the AC-14 sweep non-degenerate (R-15).

## Manifest stamp (TOML alongside fixtures — ADR-002)

```toml
# eval/corpus/fixtures/manifest.toml  (or per-corpus stamp)
manifest_version = 1            # bumped only when the HASH INPUT SET changes (distinct from migration_number)
migration_number = 47          # human legibility ONLY — NOT a hash input
shape_hash = "<64-hex>"        # SHA-256 lowercase hex of the ordered manifest (shape-hash.md computes/verifies)
```
The `shape_hash` is computed by `shape-hash.md` and stamped here; the drift guard recomputes the
running schema's hash and compares to this value (HARD ERROR on mismatch for this primary corpus).

## Assertion discipline (C-04, R-09 — enforced by loader + audit)

- EVERY scenario uses ONLY `redirect_to_head` / `forbidden_absent` / `rank_below`.
- ZERO literal-ID `expected`. ZERO null `expected`. (Loader rejects both; static audit re-asserts.)
- All anchors are aliases (e.g. `"chainA.head"`), resolved at load (`corpus-loader.md`).

## Data flow

- **Input**: authored by humans/agents following the Band-2 authoring guide (`docs.md`).
- **Output**: consumed by `corpus-loader.md` → snapshot DB; the manifest stamp consumed by `shape-hash.md`.

## Error handling

Asset-level: malformed fixtures surface as loader errors (`corpus-loader.md`). No runtime logic here.

## Key test scenarios

- **Shape coverage (AC-06)**: assert presence of multi-correction chain, dangling chain,
  superseded-Active, deprecated-connected; each loads and searches.
- **Primary-corpus audit (R-09.2 — Wave-1 backstop)**: static audit asserts ZERO literal-ID and
  ZERO null `expected`; every scenario uses only the three property types.
- **Non-degenerate (R-15.2)**: each required shape loads and yields ≥1 EVALUATED assertion against
  a non-empty result set (not vacuously satisfied).
- **Crossover bracketing (ADR-004 §5)**: the deprecated-connected shape produces a bracketed range
  of crossover points under a `clean_replacement`/multiplier sweep, not a single exemplar.
- **Dead-end head edge**: redirect-to-head against a chain with no Active terminal ⇒ defined FAIL,
  no panic.
