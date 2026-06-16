# Gate 3a Report: vnc-037

> Gate: 3a (Component Design Review)
> Date: 2026-06-16 (rework re-validation, iteration 1)
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment (7 ADRs) | PASS | All 8 pseudocode files trace to ADR-001..007; component boundaries, seams, and the sibling-module split (OQ-B) match the architecture decomposition. |
| 2. Specification coverage (19 FR, 14 AC) | PASS | Every FR-1..FR-19 has corresponding pseudocode; FR-18 cap-constant and FR-19 fail-loud both present and correct; no scope additions. |
| 3. Risk coverage (20 risks, 4 Critical) | PASS | All 20 risks map to discriminating test scenarios; the 4 Critical (R-01..R-04) carry proof-outside-cap, two-surface, and store-boundary assertions. |
| 4. Interface consistency | WARN | Shared types coherent across files EXCEPT the summary-digest `↔` tally (OQ-02) — `EdgeTotals{inbound,outbound}` cannot express a distinct symmetric count; flagged in-artifact, bounded resolution. |
| 5. Knowledge stewardship compliance | PASS | RESOLVED (rework). Pseudocode agent report now present at `agents/vnc-037-agent-1-pseudocode-report.md` with a complete `## Knowledge Stewardship` block (`Queried:` MCP-disconnected/non-blocking, ADRs+spec+risk read directly; `Deviations: none`; `Stored: nothing novel` with reason). Architect, spec, risk-strategist all previously COMPLIANT. |

## Load-Bearing Item Rulings (per spawn prompt)

### 1. Symmetric canonicalization (D-10 / ADR-007, BLOCKER) — PASS
The shared `nbr → canon → deduped` CTE (OVERVIEW.md lines 88-124) canonicalizes
`Contradicts`/`CoAccess`/`Informs` to one row **in SQL, before** any ordering or counting.
- **Ranked query** (`store-ranked-query.md` lines 33-62): builds on `deduped`, then `ORDER
  BY … LIMIT ?` — canonicalization precedes both.
- **Split count** (`store-split-count.md` lines 42-68): the byte-identical `deduped` CTE,
  then `SUM(CASE …)` — canonicalization precedes `COUNT`.
- **Two-queries-must-agree** is structurally enforced: both functions are co-located in
  `graph_queries_ranked.rs` with an explicit instruction to share the CTE as one `&str`
  fragment (store-split-count.md line 72-73; OVERVIEW.md line 24), and the test plan asserts
  `count_canon_parity_with_rank_query` independently (store-split-count.md line 33).
- Applied on **both surfaces** independently: displayed-set dedup (R-01 display) and totals
  dedup (R-01 totals / R-03) are separate asserted scenarios.
**Ruling: not a blocker — fully satisfied.**

### 2. Ranking (D-09 / ADR-006) — PASS
The exact locked clause `ORDER BY (d.source = 'agent') DESC, t.confidence DESC NULLS LAST,
target_id ASC LIMIT ?` appears verbatim (store-ranked-query.md lines 54-61, OVERVIEW.md
141-148). `LIMIT ?` is bound to `GET_EDGE_DISPLAY_LIMIT` (no literal 3). `LEFT JOIN entries t`
supplies the rank key; `weight` is explicitly NOT in the ORDER BY (store-ranked-query.md line
107). The discriminating ranking test
(`test_query_ranked_by_target_confidence_proof_outside_cap`, store-ranked-query.md lines
18-36) seeds 4 inferred edges (`GET_EDGE_DISPLAY_LIMIT + 1`) with the proof target **outside
the naive cut AND with a lower `graph_edges.weight` (0.1)** so weight-order and confidence-order
disagree — exactly lesson #3886. Per-edge rank trace is present.
**Ruling: ranking test is discriminating per #3886 — satisfied.**

### 3. OQ-02 — symmetric-count representation — WARN (NOT a blocking design gap)
**Internal-consistency analysis:**
- **JSON** (`edge_totals: {inbound, outbound}`): internally consistent. The split-count
  convention (store-split-count.md lines 28-38) buckets every `↔` edge into **inbound**, so a
  symmetric edge contributes exactly once. The locked JSON shape (ADR-005, OVERVIEW.md) is
  honored and symmetric-once. Consistent.
- **Markdown** (`### Related`): consistent. It renders the ranked ≤3 with the `↔` glyph
  per-edge and derives `N more` from `inbound + outbound` (a `↔` already counted once). No
  separate symmetric count is needed in markdown. Consistent.
- **Summary digest** (`5↑ 2↓ ↔3`): **inconsistent with the locked `EdgeTotals` shape.** The
  digest requires THREE distinct counts (asym-out, asym-in, symmetric). `EdgeTotals{inbound,
  outbound}` folds the symmetric count into `inbound` indistinguishably from asymmetric-inbound,
  so `↔3` cannot be derived from the locked totals struct.

The serializer-seam pseudocode **identifies this exact gap itself** (serializer-seam.md lines
108-119) and names two bounded resolutions: (a) add a `symmetric: usize` third aggregate to
`EdgeCountSplit`/`EdgeTotals` (a `SUM(CASE WHEN direction='both' …)` — keeps the digest honest),
or (b) render the simpler `N↑ M↓ (K authored)` form without a `↔` sub-tally.

**Ruling: WARN, not a blocker.** Reasons it is implementer-level, not a Gate-3a stop:
1. Spec FR-14 and SCOPE OQ-02 explicitly delegate the summary glyph form to the architect's
   call ("OQ-02, architect's call"); ADR-005 states it "fixes the *vocabulary*, not the final
   byte form."
2. Both named options preserve every LOCKED invariant — symmetric-once totals, the JSON
   `edge_totals` shape, and canonicalization. Option (a) is purely additive (a third count that
   never violates symmetric-once); option (b) narrows the digest. Neither relitigates a D-0x.
3. The json and markdown surfaces — the machine-consumed and primary human-read surfaces — are
   fully internally consistent. Only the summary digest's optional `↔` sub-tally is undetermined.

**However**, because the summary digest is an NFR-7 acceptance surface and AC-08 names a
`summary_digest_arrow_split` assertion with the `↔` form, the implementer MUST pick (a) or (b)
**before writing the digest test**, and if (a), `store-split-count` and `EdgeCountSplit`/
`EdgeTotals` must gain the third aggregate. Recommend the coordinator have the architect/spec
pin the digest byte form (a one-line OQ-02 decision) at the start of Stage 3b so the digest test
has a definite expected string. This is captured as the single WARN below; it does not block the
gate.

### 4. FR-19 fail-loud (AC-14) — PASS
get-edge-assembly.md lines 27-47 show edge/count/title failures after a successful primary read
map to `ServerError::Core(CoreError::Store(e))` — the **identical** mapping the primary
`entry_store.get` uses (tools.rs:963-965) — and propagate via `?`, failing the call. Opt-out
(`Some(false)`) never calls `build_edges_view`, so it cannot reach the failure (lines 27-28,
105-107). The RED test is present and named (`test_edge_query_failure_fails_loud`, run RED per
#4876) for ranked query, split count, AND title join, with the zero-vs-failure distinction
asserted (`test_zero_edges_is_success_distinct_from_failure`, get-edge-assembly test plan lines
13-29). No-unwrap static check present.

### 5. Serializer seam — PASS
serializer-seam.md confirms `entry_to_json` and `format_entry_markdown_section` signatures
UNCHANGED (lines 18-20, 123); `None ⇒ key/section never inserted` is structural, not a
guarded-omit (lines 60-63). The byte-identity golden is captured through the REAL producer (the
infra-001 MCP harness) across 4 list-view tools × 3 formats vs a pre-vnc-037 baseline
(serializer-seam test plan lines 22-26), satisfying #1268. JSON injection happens on the get
path after the base object is built (lines 52-58).

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS
**Evidence**: 8 components map 1:1 to the architecture component table. ADR-001 (rank-and-limit
in SQL + split COUNT) → store-ranked-query + store-split-count. ADR-002 (projection / discovery
list) → get-edge-vocabulary (exact 5 fields, no enrichment). ADR-003 (serializer seam) →
serializer-seam (signatures unchanged). ADR-004 (additive `source`, ranked variant JOINs
confidence) → store-neighbor-source (plain path gains `source` only) + store-ranked-query (JOIN
confined to the variant). ADR-005 (nested `edge_totals`, flat markdown, `↔`) → serializer-seam.
ADR-006 (ranking + cap constant) → store-display-cap-constant + store-ranked-query. ADR-007
(canonicalization) → the shared CTE in both store queries. The sibling-module split
(`graph_queries_ranked.rs`, `mcp/get_edges.rs`, `response/edges.rs`) matches OQ-B pre-auth.

### Check 2 — Specification coverage
**Status**: PASS
**Evidence**: FR-1 (live SQL both directions) → store-ranked-query CTE both legs. FR-2/FR-3
(include_edges, opt-out skip) → get-params + get-edge-assembly. FR-4/FR-5 (exact shape, batched
title, dangling retained) → get-edge-vocabulary + fetch_titles_batch. FR-6 (`authored` from
`source`) → store-neighbor-source + EDGE_SOURCE_AGENT exact match. FR-7 (Supersedes excluded) →
`!= 'Supersedes'` in both legs. FR-8 (canonicalization) → shared CTE. FR-9 (ranking) → locked
ORDER BY. FR-10/FR-11 (uncapped split count, bounded fan-out) → store-split-count + SQL LIMIT.
FR-12 (empty state) → render helpers. FR-13 (None⇒absent) → serializer-seam. FR-14 (three
formats) → render helpers. FR-15/FR-16 (projection / neighbors unchanged) → get-edge-vocabulary
+ store-neighbor-source. FR-17 (carried-forward authored) → assembly + neighbor-source. **FR-18
(cap constant)** → store-display-cap-constant, bound to LIMIT, referenced by render and tests, no
literal 3. **FR-19 (fail-loud)** → get-edge-assembly handler integration. No unrequested features.

### Check 3 — Risk coverage
**Status**: PASS
**Evidence**: The 4 Critical risks receive discriminating coverage: R-01 asserted on display AND
totals independently for all 3 symmetric types plus order-of-ops; R-02 proof-outside-cap with
disagreeing weight (#3886) + authored-priority + inferred-fill + tiebreak, each with a per-edge
rank trace (#3645); R-03 uncapped/parity/direction-split/nested-shape from the COUNT side; R-04
high-degree returns exactly cap rows at the **store boundary** (not rendered output). High/Medium/
Low risks all mapped (test-plan OVERVIEW risk→test table, all 20 rows). Integration harness plan
names applicable infra-001 suites and 17 new tests; fail-loud RED correctly placed as a server
integration test (no MCP failure-injection seam).

### Check 4 — Interface consistency
**Status**: WARN
**Evidence**: Shared types (`RawEdgeRow`, `EdgeCountSplit`, `GetEdge`, `EdgeTotals`, `EdgesView`,
`GetParams`) are defined once in OVERVIEW.md and used coherently per file. `RawEdgeRow.source`
populated by all paths; `target_confidence` by the ranked variant only (None elsewhere). The
direction hint flows SQL → projection without Rust re-derivation (store-ranked-query.md lines
92-101). **The one inconsistency** is the summary-digest `↔` sub-tally vs the locked
`EdgeTotals{inbound,outbound}` shape — see OQ-02 ruling above. Bounded, in-artifact-flagged,
implementer-resolvable; does not block.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS (RESOLVED in rework iteration 1)
**Rework verification**: The pseudocode agent report now exists at
`agents/vnc-037-agent-1-pseudocode-report.md` (created 2026-06-16T20:53, after all
pseudocode files at 20:42-20:47 — confirming no design artifact was touched during rework).
Its `## Knowledge Stewardship` block (lines 39-51) is complete:
- `Queried:` — context tooling unavailable (Unimatrix MCP disconnected), non-blocking per
  protocol; the 7 ADRs + SPECIFICATION.md + RISK-TEST-STRATEGY.md read directly as the
  authoritative source. Satisfies the read-only-tier `Queried:` requirement (same posture
  the spec agent used).
- `Deviations: none` with the established priors enumerated (vnc-020 additive `Option<T>`,
  crt-034/vnc-015 constant co-location, `fetch_nodes_batch` bind precedent, store-boundary
  `try_get`/`StoreError` mapping, tools.rs:963 FR-19 reuse).
- `Stored: nothing novel (read-only tier; MCP disconnected regardless)` — explicit with
  reason; no WARN.
This was the sole gating failure in the prior run; it is now satisfied.

**Original-run evidence (retained):**
- **Architect** (`agents/vnc-037-agent-1-architect-report.md`): `## Knowledge Stewardship`
  present with `Queried:` (context_briefing) and `Stored:` (#5018, #5019 via context_store;
  #5009-#5013 corrected via context_correct). COMPLIANT (active-storage tier).
- **Spec** (`agents/vnc-037-agent-2-spec-report.md`): `## Knowledge Stewardship` present with
  `Queried:` (no results — MCP unavailable, non-blocking) and read-only-tier no-storage
  rationale. COMPLIANT.
- **Risk-strategist** (RISK-TEST-STRATEGY.md `## Knowledge Stewardship`): `Queried:`
  (context_search — found #3886/#3645/#3621/#1044/#4166/#4247) and `Stored: nothing novel to
  store -- {reason}` with a specific reason (#3886 already covers the pattern). COMPLIANT
  (active-storage tier — explicit decline with reason).
- **Pseudocode agent**: **NO agent report and NO `## Knowledge Stewardship` block anywhere.**
  Neither the pseudocode/ files nor any report under agents/ carry a `Queried:` entry for the
  pseudocode work. The Gate 3a rule is explicit: read-only agents (pseudocode) must have
  `Queried:` entries; **missing stewardship block = REWORKABLE FAIL.**
**Issue**: The pseudocode agent must record its `## Knowledge Stewardship` block (with at least
a `Queried:` entry — or a documented "MCP unavailable, non-blocking" if that was the case, as
the spec agent did) in an agent report under `agents/` (or appended to the pseudocode OVERVIEW).
This is the only gating failure.

## Rework Required — RESOLVED (iteration 1)

| Issue | Which Agent | Status |
|-------|-------------|--------|
| Missing pseudocode-agent `## Knowledge Stewardship` block (no `Queried:` entry anywhere) | uni-pseudocode | RESOLVED — `agents/vnc-037-agent-1-pseudocode-report.md` added with a complete stewardship block (`Queried:` MCP-disconnected/non-blocking; `Deviations: none`; `Stored: nothing novel` with reason). No design artifact modified. |

## Advisory (WARN — not gating; resolve before Stage 3b digest test)

| Item | Which Agent | What to decide |
|------|-------------|----------------|
| OQ-02 summary-digest `↔` sub-tally form | uni-architect (or spec) | Pin the summary digest byte form: option (a) add a `symmetric: usize` aggregate to `EdgeCountSplit`/`EdgeTotals` (a `SUM(CASE WHEN direction='both')` in store-split-count) to render `N↑ M↓ ↔K`, OR (b) render `N↑ M↓ (K authored)` with no `↔` sub-tally. Either preserves symmetric-once totals + JSON shape. Needed so the AC-08 `summary_digest_arrow_split` test has a definite expected string. If (a), store-split-count + the two structs must gain the third count. |
