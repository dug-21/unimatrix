# vnc-044 Retrospective — Architect

Mode: retrospective. Feature shipped (PR #920 merged, #913 closed). No new design produced.

## 1. Patterns

- **Updated #5511 → #5523** — cleaned a malformed entry. The stored content had a leaked
  tool-call fragment (`</content>` + `<parameter name="tags">…`) bled into the body and an
  empty tags field. Corrected the content and moved the tag list into the tags field; substance
  unchanged. The pattern itself (projection change over N sibling handlers under-tested when the
  incident cites one) is high-quality and generic — confirmed as evidence.
- **New #5524** — "Never byte-compare two live server reads of a payload carrying
  background-mutable fields (confidence/access_count/last_accessed_at)." The Gate-3c flake, raised
  to a reusable testing pattern. NOT covered by #5521 (default-flip + dedup fixture) — orthogonal
  third trap on the same two-axis migration path, directly relevant to #922.
- **Confirmed, no change:**
  - #5505 (lifecycle-status vs delivery-status trap) — substantive what/why/scope, generic to any
    capability-status surfacing feature.
  - #5520 (rustfmt `--edition 2024 --config skip_children=true` in unimatrix-server) — real
    tooling gotcha with a why; reusable for any server-crate single-file format. Not API-docs.
  - #5521 (two-axis default-flip test breakage + >0.9 dedup fixture collapse) — forward-looking,
    names next adopters; kept as-is.
- **Skipped (with reason):**
  - "Graph-local resolver instead of shared parse_format" — already captured in ADR-002 (#5510)
    §2, which every migration adopter reads. Storing separately duplicates the ADR.
  - "Serialization-seam trait per envelope, traversal unchanged" — the #4500 per-mode-coordinated
    change pattern already exists and is cross-referenced by the ADR.

## 2. Procedures

- **None.** The harness `client.py` `detail` param addition (mirror the existing `format`
  arg-marshalling; add `detail: str | None = None`) is a near-trivial one-liner discoverable from
  the existing `format` handling, and the adopter guidance already lives in #5521 + the ADRs. Not
  worth a standalone procedure.

## 3. ADR status

- **ADR-001 (#5509) — VALIDATED, not flagged.** Implementation honored the contract end-to-end
  (Gate 3b/3c PASS: single-sourced `CONTENT_PREVIEW_BYTES`, mandated char-boundary idiom, summary
  field set, default `full`→`summary`, lifecycle-status disclosure). Known caveat stands: the
  contract's per-tool field-set override and general applicability are **provisional until a 2nd
  adopter** (context_get / context_search / context_cycle_review under #922) exercises them.
  context_graph is a single data point. No supersession.
- **ADR-002 (#5510) — VALIDATED, not flagged.** Resolver-before-dispatch, distinct
  `graph_read_projection.rs`, shared types untouched, byte-identical `Detail::Full`, additive
  `GraphParams.detail` all confirmed in code. One implementation refinement — a generic
  `serialize_detail<T>` helper replacing the pseudocode's per-arm inline match — is a DRY
  improvement **within** the contract (Full arm still serializes the raw envelope; byte-identical),
  not a deviation. No ADR change warranted.

## 4. Lessons

- **New #5524** (stored as a pattern — reusable test-authoring gotcha, not a feature-specific
  postmortem, so it does not poison lesson-recall).
- **Skipped: Gate-3a `parse_detail` case-policy conflict.** Considered the thin lesson "a new
  parser introduced alongside an existing sibling (parse_format) must single-source its policy or
  parallel Stage-3a agents diverge." Does not clear the bar: it is a weak instance of the known
  "parallel agents must single-source shared decisions" principle, the concrete fix (mirror the
  `parse_format` idiom) is what a grep of the codebase reveals by default, and it was corrected
  in-cycle. Storing it would be low-value clutter.
- **Not stored (per rule):** the post-merge auto-filed follow-up issues (#921–#924 without human
  agreement, #921 closed on objection) is leader behavior, not Unimatrix knowledge.

## 5. Retrospective findings

- **Contamination confirmed** — the mutation_spread (102 files) / design_artifact_count (80) /
  adr_count (5) hotspots are inflated by ~150 nxs-014 artifacts from an overlapping session. vnc-044's
  real footprint is the 4 server components + test suites. Discounted; no action.
- **file_breadth (138 files, test phase)** — expected for the Stage-3c tester (harness + integration
  suites + reading task outputs). Not a lesson.
- **tool_failure_hotspot (8× Bash over ~6h, path typos on report writes)** — low signal, no action.
- **Outlier `context_load_before_first_write_kb` 4258 vs 1013 (2σ)** — expected for a combined
  design→delivery leader arc; note only.
- **Real rework signals** — both captured: Gate-3a case-policy divergence (skipped as a lesson,
  above) and Gate-3c live-read flake (→ #5524). Telemetry undercounted to "1 loop"; there were two
  distinct REWORKABLE FAILs, both test-only, production code correct throughout.
- **Transcript candidates** — all `provenance: reconstructed` (0.81 floor), `search_complete: false`;
  weighted low, no decisions extracted. Indeterminate, not negative.

## 6. Relationship edges

**None — bar not met.**
- ADR-002 (#5510) → ADR-001 (#5509) `Prerequisite` **already exists** (confirmed in #5510's edges).
- ADR-001 (#5509) → crt-057 #5434 `Prerequisite` **already exists**.
- New pattern #5524 warrants no edge: a future agent does not need to *traverse* from it to a
  decision to avoid a wrong choice — it is discoverable by testing-domain search. Supports/
  Prerequisite/Contradicts all fail the traversal-necessity test.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search "vnc-044" (k=20) + context_get on #5505/#5509/#5510/#5511/#5520/#5521 — reviewed all cycle stores against category templates.
- Stored: entry #5524 "Never byte-compare two live server reads of a payload carrying background-mutable fields" via context_store (new cross-feature testing pattern; the Gate-3c flake, not covered by #5521).
- Corrected: #5511 → #5523 via context_correct (stripped leaked tool-call XML fragment from content body, moved tag list into the tags field; substance unchanged).
- Confirmed as evidence, no change: #5505, #5509, #5510, #5520, #5521.
- Edges: none asserted — the ADR Prerequisite spine already exists and #5524 fails the traversal-necessity bar.
