# Component: Fixture corpus loader + property assertions — `corpus-loader.md`

**Wave**: 1
**Location**: `crates/unimatrix-server/src/eval/corpus/{loader.rs, assertions.rs, mod.rs}` (new).
**ADR**: ADR-004 (#4898). **Risks**: R-09 (literal/null regress), R-10 (alias resolution), R-15 (depth).

## Purpose

Materialize hand-authored fixture entry-graphs (TOML, `corpus-fixtures.md`) into a snapshot DB +
vector dir that the EXISTING `EvalServiceLayer::from_profile(db_path, ..)` consumes unchanged
(corpus = just another snapshot source — cumulative test infra). Produce an `AliasMap`
(alias → resolved id + head-member sets) for trust evaluation. Reject literal-ID / null `expected`
in the primary corpus.

## On-disk fixture format (authored; see `corpus-fixtures.md` for assets)

```toml
# one fixture entry-graph file
[[entries]]
alias = "chainA.head"          # stable handle; NEVER a literal id in assertions
title = "..."; content = "..."
status = "Active"              # Active | Deprecated
superseded_by = []             # alias references (resolved at load) — NOT ids
category = "..."

[[scenarios]]
query = "..."
# assertions are property-based ONLY (no `expected` literal-ID list in primary corpus):
[scenarios.assertions]
redirect_to_head = ["chainA.head"]
forbidden_absent = ["chainA.stale"]
rank_below       = [["chainA.b", "chainA.head"]]
```

## Loader pipeline

```
pub fn load_fixture_corpus(dir: &Path, target_db: &Path) -> Result<LoadedCorpus, CorpusError> {
    // 1. Parse all fixture TOML files under `dir`.
    let raw = parse_fixtures(dir)?;                      // path-traversal check on any file ref (security)

    // 2. Reject forbidden `expected` forms in the PRIMARY corpus (R-09, C-04).
    for scenario in raw.scenarios:
        if scenario.has_literal_expected():    return Err(CorpusError::LiteralIdExpected { scenario })   // banned
        if scenario.assertions.is_none() && scenario.expected.is_none():
                                               return Err(CorpusError::NullExpected { scenario })        // banned
        // primary corpus MUST carry `assertions`; never a literal `expected`.

    // 3. Validate aliases: global uniqueness (R-10).
    let mut alias_to_id: Map<alias, id> = {}
    let mut next_id = BASE_ID
    for entry in raw.entries:
        if alias_to_id.contains(entry.alias):  return Err(CorpusError::DuplicateAlias { alias })          // hard error
        alias_to_id[entry.alias] = next_id; next_id += 1

    // 4. Resolve `superseded_by` alias refs to ids; build the entry rows + Supersedes edges.
    for entry in raw.entries:
        for succ_alias in entry.superseded_by:
            succ_id = alias_to_id.get(succ_alias)  else return Err(CorpusError::MissingAlias { alias: succ_alias })
            add Supersedes edge (entry_id -> succ_id)

    // 5. Validate every assertion alias resolves (R-10) — missing ⇒ hard error, NEVER vacuous pass.
    for scenario in raw.scenarios:
        for aref in scenario.all_assertion_aliases():
            if not alias_to_id.contains(aref):  return Err(CorpusError::MissingAlias { alias: aref })

    // 6. Materialize the snapshot DB + vector dir (embed-at-load is safe — branch (b), shape-hash.md).
    write_entries_and_edges_to_sqlite(target_db, rows, edges)?      // writes ONLY under controlled path
    embed_and_write_vectors(target_db_vector_dir, rows)?           // ONNX embed; protected by shape hash

    // 7. Precompute head-member sets for redirect_to_head (find_terminal_active semantics, graph.rs:547).
    head_members: Map<EntryRef, Set<id>> = {}
    for head_alias in all redirect_to_head aliases:
        head_id = alias_to_id[head_alias]
        // members = entries whose terminal-active resolves to head_id (its superseded predecessors)
        head_members[head_alias] = { e.id for e in rows if find_terminal_active(e.id, graph, rows) == Some(head_id) && e.id != head_id }

    Ok(LoadedCorpus {
        db_path: target_db,
        alias_map: AliasMap { alias_to_id, head_members },
    })
}
```

## `AliasMap`

```
pub struct AliasMap {
    alias_to_id: Map<EntryRef, u64>,
    head_members: Map<EntryRef, Set<u64>>,    // for redirect_to_head evaluation
}
impl AliasMap {
    pub fn resolve(&self, r: &EntryRef) -> u64 { self.alias_to_id[r] }   // total — load guaranteed existence
    pub fn head_members(&self, head: &EntryRef) -> &Set<u64> { &self.head_members[head] }
}
```
Resolution is total at evaluation time BECAUSE load already proved every assertion alias exists
(step 5). No path where a missing alias degrades to a silent vacuous pass (R-10).

## Property assertions (`assertions.rs`)

Defines the on-disk `ExpectedAssertions` shape (shared with `trust-metric.md`) and the deserializer.
The evaluation lives in `trust.rs::evaluate_trust`; this module only parses + resolves anchors.
The three property types — redirect-to-head, absence, rank-below — are operationally defined in
`trust-metric.md`.

## Snapshot reuse boundary (R-15 integration seam)

The loader produces a snapshot DB the UNCHANGED `EvalServiceLayer::from_profile` rebuilds into a
`TypedGraphState`. The replay/metric machinery is reused verbatim — the corpus is just a snapshot
source. The only nan-018-specific artifact crossing the boundary is the `AliasMap` (produced at
load, consumed by `evaluate_trust`); the replay path itself sees only ids.

## Security (R-TEST)

- Path-traversal check: any author-supplied file reference must resolve UNDER the controlled
  corpus dir; reject absolute / `../` paths.
- The materialized DB + vector dir are written only under a controlled temp/eval path.
- Malformed/oversized TOML errors cleanly (no panic/hang).

## Data flow

- **Input**: fixture TOML dir.
- **Output**: `LoadedCorpus { db_path, alias_map }`; `db_path` feeds `EvalServiceLayer::from_profile`;
  `alias_map` feeds `evaluate_trust`.

## Error handling (all HARD errors — fail loud, never silent)

| Condition | Error |
|-----------|-------|
| literal-ID `expected` in primary corpus | `LiteralIdExpected` |
| null `expected` (no assertions, no expected) | `NullExpected` |
| duplicate alias (any file) | `DuplicateAlias` |
| assertion / `superseded_by` references undefined alias | `MissingAlias` |
| path traversal in a file ref | `PathTraversal` |

## Key test scenarios

- **Loader rejection (R-09.1, AC-05)**: null `expected` rejected; literal-ID `expected` rejected.
- **Primary-corpus audit (R-09.2)**: scan shipped corpus — every scenario uses ONLY
  redirect-to-head / absence / rank-below; zero literal-ID, zero null. (See `corpus-fixtures.md`.)
- **Renumber survival (R-10.1)**: load twice with different id assignment; every alias-based
  assertion resolves to the same logical entry and same pass/fail verdict.
- **Missing alias (R-10.2)**: assertion referencing an undefined alias ⇒ hard load error.
- **Duplicate alias (R-10.3)**: same alias twice ⇒ rejected.
- **head-member precompute**: redirect_to_head members correctly enumerate superseded predecessors
  whose terminal-active is the head.
- **Snapshot reuse**: loaded DB consumed by `EvalServiceLayer::from_profile` unchanged; search replay runs.
- **Path traversal**: a `../` file ref rejected.
