# Test Plan — Component: discoverable-contract doc surfaces

Component: four edit points documenting `edge_types`/`direction` availability on subgraph + the
depth-1-live/depth>1-cache staleness carve-out.

Edit points (R-07 / SR-01 / AC-13 / AC-09):
1. `direction` schemars doc — `graph_read.rs:82` (currently `chain: …; neighbors: …`, no subgraph)
2. `edge_types` schemars doc — `graph_read.rs:84` (currently "neighbors only: …")
3. `CONTEXT_GRAPH_DESCRIPTION` mirror const — `tools.rs:76`
4. live `#[tool(description=…)]` literal — `tools.rs:~3945–3996` (byte-identical twin of #3)

Points 3+4 are guarded by `test_graph_tool_attr_description_matches_const` (#869, `tools.rs:6263`).
Points 1+2 (schemars docs) are NOT covered by that guard — they need an explicit presence check.

Home suite: `tools.rs` `#[cfg(test)]` module; `graph_read.rs`/`graph_read_tests.rs` for the schemars
check.

---

## R-07 — Four-point doc drift (Critical) → AC-09, AC-13

### Points 3+4 (the two description literals)

- **test_graph_tool_attr_description_matches_const** (existing, `tools.rs:6263`) — MUST stay green
  after both literals are edited identically. This is the byte-equality guard (#869/#5396); it is the
  structural drift prevention for the two copies. No new test — the existing one is the gate. Editing
  only three of four points, or diverging the two literals, red-bars here.

- **test_context_graph_description_contains_staleness_text** (existing, `tools.rs:6184`) — EXTEND the
  substring assertions to require the NEW phrases (asserted against the `CONTEXT_GRAPH_DESCRIPTION`
  mirror const, which the byte-equality guard ties to the live literal):
  - Filter-availability (AC-13): a phrase stating `edge_types`/`direction` are honored on subgraph
    mode (e.g. `description.contains("subgraph")` co-located with `edge_types`/`direction` filtering
    text — the coder picks the exact wording; the test pins its presence so it cannot silently drop).
  - Staleness carve-out (AC-09): depth-1 = live DB (all committed writes visible); depth>1 = tick-cache
    (30–60s). Reuse/extend the existing `depth=1 queries the live database` /
    `depth>1 queries the in-memory graph cache` / `tick interval` assertions to cover subgraph, not
    just neighbors, if the current text is neighbors-scoped.
  - Existing negative assertion `!description.contains("graph_rebuilt_at")` stays (no freshness field).
  - **Coder + tester agree the exact substring literals** so the assertion and the description text are
    edited together in one PR (avoids a self-inflicted red-bar).

### Points 1+2 (the two schemars docs — NOT byte-equality guarded)

- **test_graphparams_schemars_docs_state_subgraph_applies** (ADD) — assert the field descriptions in
  the generated schema mention subgraph applicability and drop "neighbors only":
  - Preferred: use `schemars::schema_for!(GraphParams)`, read the `direction` and `edge_types` property
    `description`, assert each contains "subgraph" and that `edge_types`'s no longer reads "neighbors
    only". This is a real schema-doc assertion (survives doc-comment refactors), and confirms the
    discoverable JSON schema — the surface an agent actually reads — is corrected.
  - Fallback if `schema_for!` on `GraphParams` is awkward (custom `#[schemars(with=…)]` on some fields):
    an in-crate source/doc-string presence check. Either way, BOTH schemars docs get an explicit,
    non-manual verification — no point left to manual sync.

- **Coverage**: all four edit points verified — two literals via the byte-equality guard + extended
  substrings; two schemars docs via an explicit presence/schema-doc assertion.

## FR-5 / Open Q4 — snapshot pin discovery (resolved negative)

Architecture verified NO `.snap`/`insta`/`assert_snapshot`/`schema_for`-snapshot pins the description
string or the `GraphParams` schema. Execution step (3c): confirm this still holds (quick grep for
`insta`, `assert_snapshot`, `.snap` under `crates/unimatrix-server/`) and record the negative in
RISK-COVERAGE-REPORT as AC-10 evidence. If a snapshot appears, update it in-scope — do NOT let a stale
snapshot silently red-bar CI.

## Wire-level doc verification (optional, low priority)

The infra-001 `protocol` suite discovers tool descriptions via `tools/list`. No new wire test is
required for the doc fix (the in-crate substring + schema assertions are authoritative), but the
existing tool-discovery smoke test transitively exercises that the description still serializes.

## AC coverage from this component

AC-09 (staleness text present in both literals), AC-13 (four edit points; two-copy same-body invariant
+ schemars-doc presence). AC-10 partial (snapshot-absence confirmation).
