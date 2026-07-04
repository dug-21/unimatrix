# Test Plan — Consumer Reconciliation (5-site atomic unit) + Ownership Boundary (AC-19)

**Files:** `uni-retro/SKILL.md`, `context_cycle_review` tool description (`mcp/tools.rs`),
`uni-delivery-protocol.md`, `uni-bugfix-protocol.md` (the protocol restructure is tested in
`retro-lifecycle.md`); server response schema for AC-19.
**Risks:** R-02 (Critical, dominant scope risk) · **ACs:** AC-16, AC-19

> The atomic unit is server + `uni-retro/SKILL.md` + tool description + BOTH protocols. Shipping the server
> alone silently starves every candidate consumer (harvest #5219) with NO error — the failure most likely to
> escape code-level tests. A green server-only suite MUST NOT pass this gate.

---

## R-02 — reconciliation guard (AC-16)

### Grep guard over the CORRECTED four-doc set
- `test_no_residual_boolean_or_purge_language` — assert NONE of `include_transcript_candidates`,
  "any review carries candidates", "review purges" survives in `uni-retro/SKILL.md`, the tool description, or
  either protocol file. (AC-16, R-02 sc.2.)
- `test_transcript_block_referenced_in_consumers` — assert `uni-retro/SKILL.md` and the tool description
  reference the `transcript{}` block (the retro's candidate call is the `transcript:{}` full block).
- `test_tool_description_states_no_purge_verb` — assert the tool description states plainly the tool has NO
  purge verb and lists the three orthogonal axes (`format` render-only, `force` recompute-only,
  `transcript{}` read-only scoped retrieval). (R-02 sc.3.)
- `test_retro_skill_is_repeatable_not_one_shot` — assert `uni-retro/SKILL.md` no longer sequences around a
  one-shot extraction (retrieval is non-destructive and re-runnable in any scope). (R-02 sc.4.)

**EXCLUSION (non-negotiable):** the grep set is EXACTLY those four docs. **`uni-agent-routing.md` is NOT
grepped** — a passive descriptive mention that no live protocol loads; a guard on it FAILS spuriously
(IMPLEMENTATION-BRIEF explicit). Add an assertion that `uni-agent-routing.md` is absent from the checked set.

### End-to-end harvest-fires reachability (#5383 — not a store read-back)
- `test_reconciled_retro_call_delivers_candidates_and_loss` — drive the reconciled `/uni-retro` path (the
  `transcript:{}` block, NOT the removed boolean) against the running server and assert the response actually
  contains a candidates section WITH per-session `SessionLossInfo`. Proves consumer + server agree
  post-change (R-02 sc.1). This is the load-bearing reachability check — a store-layer read-back is a false
  positive.

---

## AC-19 — Ownership Boundary (NG-5) — DEDICATED NEGATIVE TEST (least-verified AC)

> ACCEPTANCE-MAP flags AC-19 as having **no dedicated risk scenario** (R-18 sc.1 is only *adjacent* — it
> proves no transcript signal enters the summary, NOT that the schema lacks an attribution/ledger field).
> This plan adds the dedicated negative coverage the map recommends. Do NOT lean on R-18.

- `test_response_schema_has_no_attribution_field` — serialize a full `context_cycle_review` response
  (Plane-A summary + scoped Plane-B slice) and assert the schema carries **NO** attribution / cross-source-join
  / human-ledger field: no synthesized GH `## Knowledge Stewardship` join, no applied-entry attribution, no
  rework-count↔cause join, no human-intervention ledger. (Compile-time struct-shape + serialized-form
  assertion, mirroring `test_candidates_structurally_absent_from_memoized_report`.)
- `test_no_code_path_synthesizes_across_gh_blocks` — source/behavioral assertion that no code path in the
  handler joins or synthesizes across GH stewardship blocks. The tool returns the Plane-A summary + the honest
  scoped Plane-B slice (candidates + `SessionLossInfo`) ONLY — never a causal claim, never a cross-plane join.

**Coverage requirement:** AC-19 is verified by a schema-shape assertion AND a code-path assertion — a
negative requirement fencing scope OUT, proven by construction, not by inspection alone.

## Notes
- The protocol half of the atomic unit (merge→close→retro, both files) is tested in `retro-lifecycle.md`
  (AC-17). AC-16 fails if any consumer implies old semantics; AC-17 fails if either protocol omits the order.
