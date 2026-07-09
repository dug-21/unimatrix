# C8 — Review handler populate `report.tags`

**File:** `crates/unimatrix-server/src/mcp/tools.rs` (cycle_review handler, ~:3409-3428, beside the goal populate)
**ADR:** ADR-004. **Risks:** R-03, R-04.3. **AC:** AC-05 (JSON), degrade parity.

## Purpose

At review time, read the frozen tag set fresh from `cycle_tags` (C3) and populate `report.tags` so it
serializes into both the JSON output and `summary_json`. Degrade to `[]` + `tracing::warn` on read
error — review NEVER fails on tag read (parity `get_cycle_start_goal` degrade arm :3425).

## Testability seam — REQUIRED (rmcp `#[tool]` is not unit-constructible, entry #5389)

`context_cycle_review` is an rmcp `#[tool]` (`tools.rs:2406/:2409`) whose handler needs a live
`RequestContext<RoleServer>` that CANNOT be constructed in unit scope (#5389). The codebase's
established response is to extract handler logic into module-scope `pub(crate)` seam fns that tests
call directly (existing examples: `current_phase_for_session` :552, `finalize_note` :610,
`derive_namespace` :651, `check_tag_lifecycle` :669; and the col-024 three-path logic was extracted
"from the context_cycle_review closure" for exactly this reason, :8755).

For AC-05 to be proven on the assembled path with the REAL `get_cycle_tags` read (not a hand-built
`RetrospectiveReport` literal), the tag-populate MUST be an extracted `pub(crate)` seam:

```
# module scope in tools.rs (NOT inside the #[tool] handler), unit-callable:
pub(crate) async fn populate_review_tags(
    store: &Store,               # same store handle the handler holds
    feature_cycle: &str,
    report: &mut RetrospectiveReport,
) {
    report.tags = match store.get_cycle_tags(feature_cycle).await {
        Ok(tags) => tags,                          # deterministic, ORDER BY tag (C3)
        Err(e) => {
            tracing::warn!("vnc-047: get_cycle_tags failed for {feature_cycle}: {e}");
            Vec::new()                             # degrade — review still succeeds
        }
    };
}
```

The `#[tool]` handler then calls the seam (one line) beside the existing goal populate:

```
# 10i (continued): beside `report.goal = …` / after the goal match block.
# feature_cycle and store are already in scope here (used for get_cycle_start_goal).
populate_review_tags(store, &feature_cycle, &mut report).await;
```

- Keep the seam narrow: it does the read + degrade + assign ONLY. Do NOT refactor the goal populate
  into it (goal is out of scope; leave :3409-3428 as-is except adding the tag call).
- The seam takes `&Store` (the concrete store handle the handler already dereferences at :3411), so it
  is callable from a store-backed integration test without any rmcp plumbing.

## Seam-extractability: CONFIRMED (with one scoped flag)

**Confirmed cleanly seam-extractable.** The tag-populate is a pure `store.get_cycle_tags(fc)` +
degrade-to-`[]` assign — no `RequestContext`, no identity/cap/format state — so it lifts into
`pub(crate) async fn populate_review_tags(&Store, &str, &mut RetrospectiveReport)` with no
dependencies on the un-constructible handler shell. The AC-05 assembled test then:
1. drives `context_cycle(start, tags=[…])` through hook → listener → `cycle_tags` (real write, C4/C5);
2. calls `populate_review_tags(store, fc, &mut report)` → REAL `get_cycle_tags` into `report.tags`;
3. calls `render_tags_section(&report)` (C9, already a directly-callable module fn) + `serde_json`
   → asserts tags in BOTH markdown and JSON. This is an assembled read+render, not a store-only test.

**Flag (scoped, not blocking):** this test bypasses the rmcp `#[tool]` wrapper itself (identity
resolution, `Capability` gate, format selection, memoization) — that wrapper is NOT exercised by the
AC-05 path because it is not unit-constructible (#5389). The wrapper is covered separately by the
AC-06 handler-registry / auth tests and the existing crt-033 memoization integration tests
(`build_cycle_review_record`, :9975+). No fuller in-process rmcp-service harness exists at HEAD; if
one is later added, the AC-05 test could drive the tool end-to-end and the seam becomes an
internal-only detail. Recommend the tester cite `populate_review_tags` + the assembled write path in
`proven_by` for AC-05 (SR-08).

### Source-of-truth discipline

- Read `cycle_tags` FRESH each review via C3. Do NOT trust a prior `summary_json` mirror — the
  `summary_json` copy is display-only; trusting it over `cycle_tags` would be a source-of-truth
  inversion (RISK-TEST §Integration Risks).
- The populated `report.tags` then rides the existing `build_cycle_review_record` → `serde(report)` →
  `summary_json` path (:4554) with no change needed there (automatic serialization, C7).

## Data flow

- **Input:** `feature_cycle` (in scope), `store`.
- **Output:** `report.tags: Vec<String>` (empty when no tags or on read error).
- Downstream: JSON via serde (automatic); markdown via C9 `render_tags_section(report)`.

## Error handling

- `get_cycle_tags` error → `report.tags = []` + `tracing::warn`; the review response is produced
  normally (degrade parity). No caller-visible failure.

## Key test scenarios (hints)

1. **Assembled (AC-05, R-03):** start a cycle with tags via the hook path, then `context_cycle_review`
   → tags appear in BOTH JSON and markdown (this — not a store getter — is the AC-05 proof, SR-08).
2. Tag-less cycle → `report.tags == []`; JSON shows `"tags": []`; markdown shows the no-tags line (C9).
3. `get_cycle_tags` error injected → `report.tags == []` + warn; review still returns 200/Ok (R-04.3).
4. Source-of-truth: after tags exist in `cycle_tags`, review reflects them even if an older
   `summary_json` mirror lacked them.
