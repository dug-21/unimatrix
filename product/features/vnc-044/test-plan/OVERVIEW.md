# vnc-044 Test Plan — Overview

> `context_graph` two-axis split: `format` (serialization `markdown|json`) + `detail` (verbosity `summary|full`) with a lean node projection. Rooted in RISK-TEST-STRATEGY.md (R-01..R-14) and ACCEPTANCE-MAP.md (AC-02..AC-09, AC-03b). Component test plans map 1:1 to the pseudocode component files.

## Test Strategy

Three layers, weighted to where the risk lives:

| Layer | Where | What it proves | Primary risks |
|-------|-------|----------------|---------------|
| **Unit** | Rust `#[cfg(test)]` in each new/edited module | Pure logic: UTF-8 flooring, truncated-flag byte-compare, projection field sets, resolver decision table | R-01, R-02, R-07, R-08, R-05 (resolver), R-12 |
| **Integration (MCP)** | `product/test/infra-001/suites/` — compiled binary via JSON-RPC | End-to-end axis threading, per-mode projection through the wire, golden byte-equality, uniform markdown rejection, size win | R-03, R-04, R-05, R-09, AC-02/AC-05/AC-06/AC-08 |
| **Static / review** | `cargo test --workspace --no-run`, grep, code-review, doc-review | Shared-type non-regression, single-source constant, lifecycle-vs-delivery caveat present | R-06, R-11, R-12, R-13 |

**Why this split.** The two DoS-class Critical risks (R-01 UTF-8 flooring, R-02 truncated flag) are pure functions on `content_preview(&str)` — fastest and most exhaustively covered as table-driven **unit** tests in `verbosity.rs`. The threading/dispatch Critical risks (R-03 five-mode projection, R-04 full byte-equality) are only observable **through the wire** — they need MCP integration tests, because a unit test cannot prove the resolved axis reached the serializer past the `graph_read.rs:251` parse-and-drop seam. R-06 (shared-type non-regression) is a **compile + review** gate, not a behavioral test.

## Risk → Test Mapping

| Risk | Priority | Covered by | Component plan | AC |
|------|----------|-----------|----------------|-----|
| R-01 UTF-8 char-boundary flooring | **Critical** | Unit: `content_preview` boundary table (empty / <256 / =256 / 257-ASCII / multibyte-straddle-256/257/258 / boundary-exact); property no-panic | verbosity.md | AC-03b |
| R-02 `content_truncated` byte-compare | **Critical** | Unit: `truncated == content.len() > 256`, both sides of 256, 257-floors-to-256 trap | verbosity.md | AC-03b |
| R-03 default-summary all 5 node modes + metadata preserved | **Critical** | Integration: default + explicit summary per mode; unit: per-envelope `to_summary_json` metadata | graph_read_projection.md, graph_read.md | AC-05 |
| R-04 `detail=full` byte-for-byte | **Critical** | Integration: golden byte-equality `subgraph` + ≥1 other mode | graph_read.md | AC-04 |
| R-05 `format=markdown` rejected all 7 modes | High | Integration: markdown on each of 7 modes; unit: `resolve_graph_output` markdown arm | graph_read.md | AC-08 |
| R-06 shared types unchanged | High | Static: `--no-run` compile, grep no `skip_serializing_if`, non-graph regression suite green | graph_read_projection.md, graph_read.md | NFR-2 |
| R-07 exact field set (present AND absent) | High | Unit: `NodeSummary` + edge projection key-set; Integration: through-wire key-set | graph_read_projection.md | AC-03 |
| R-08 legacy `format=summary` alias + conflict | Med | Unit: resolver alias/conflict table; Integration: alias equivalence | graph_read.md | AC-07 |
| R-09 `detail` accept-and-ignore neighbors/path | Med | Integration: identical output across detail values; unit: no validation rejection arm | graph_read.md, graph_read_projection.md | AC-08 |
| R-10 empty/boundary content, tags, confidence | Med | Unit: empty-content projection, tag preservation, confidence number | graph_read_projection.md, verbosity.md | AC-03 |
| R-11 lifecycle-vs-delivery status | Med (doc) | Review: tool-description + AC-06 caveat; optional behavioral illustration | tools.md | AC-06, AC-09 |
| R-12 `256` single-sourced | Low | Static: grep no bare `256` in graph path; tests reference `CONTENT_PREVIEW_BYTES` | verbosity.md | — |
| R-13 assertion strings vs running strings | Low | Discipline: substring assertions on error copy, not verbatim | graph_read.md, tools.md | AC-08 |
| R-14 projection file placement / line budget | Low | Static: file-size review | graph_read_projection.md | NFR-5 |

## Cross-Component Test Dependencies

- `content_preview` / `CONTENT_PREVIEW_BYTES` (verbosity.rs) are consumed by `node_summary` (graph_read_projection.rs). Unit-test the primitive **in isolation first** (verbosity.md), then the projection uses it — projection tests assume the primitive is proven and do not re-test the full boundary table.
- `resolve_graph_output` (graph_read.rs) gates every mode arm; its decision table is unit-tested in graph_read.md. The projection trait impls (graph_read_projection.md) are tested independently of the resolver, then the two meet at the MCP integration layer (R-03/R-04).
- Golden byte-equality (R-04) depends on `detail=full` NOT routing through the projection — a graph_read.md integration concern that transitively guards graph_read_projection.rs did not leak into the full arm.

## Integration Harness Plan (infra-001)

### Suites that apply (per suite-selection table)

vnc-044 touches **server tool logic** + **store/retrieval projection behavior** + **schema-adjacent serialization** → run:

| Suite | Why | New tests? |
|-------|-----|-----------|
| `smoke` (`-m smoke`) | Mandatory minimum gate | Add 1 smoke: `context_graph(subgraph, detail=summary)` returns lean node |
| `test_tools.py` | `context_graph` is the 14th tool; all axis parameter behavior lives here (vnc-018 precedent at `:3848+`) | **YES** — new `detail`/`format` axis tests, markdown rejection ×7 modes, alias/conflict, accept-and-ignore |
| `test_lifecycle.py` | vnc-019 subgraph lifecycle tests at `:3292+`; per-mode default-summary + envelope-metadata preservation, golden full byte-equality | **YES** — 5-mode summary projection + full golden |
| `test_protocol.py` | tool-count / handshake unaffected but must stay green (`:37` asserts 14 tools) | No new — regression only |
| `test_get_edges.py` | edge projection field set through the wire | Possibly — confirm edge `{source_id,target_id,relation_type,depth}` present, `direction`/`metadata` absent |

Not run for behavior: `security`, `contradiction`, `confidence`, `volume` — untouched by vnc-044 (traversal/scoring/scan semantics unchanged, NFR-6). `volume` runs only as part of a full-suite regression pass if time permits; it is not a gate for this feature.

### Required harness extension (cumulative — extend, do not scaffold)

`harness/client.py:746 context_graph(...)` currently exposes `format` but **NOT** `detail`. Stage 3c MUST add `detail: str | None = None` to the method signature and its arg-marshalling block (mirroring the existing `format` handling at `:771`). This is the single harness change; all new integration tests depend on it. Do not build a parallel graph client.

### Integration-level scenarios to validate (through the wire)

1. **Axis threading (AC-02, R-03 seam):** same subgraph query, `detail=summary` vs `detail=full` → structurally different payloads (proves the resolved value is threaded past `:251`, not re-dropped).
2. **Default-summary per mode (AC-05, R-03):** for each of `subgraph`/`chain`/`current`/`inverse`/`filter`, a call with **no** `detail` returns lean nodes and equals the explicit `detail=summary` output; each asserts its own preserved envelope metadata (`truncated`/`seed_ids`/`depth_reached`; `total_returned`; `Truncated`; single-node for `current`).
3. **Full byte-equality (AC-04, R-04):** capture `detail=full` output for a fixed `subgraph` (+ one other mode) query; assert byte-identical to a golden captured from the pre-vnc-044 binary, OR — if a pre-change capture is impractical — assert the full arm's JSON parses to the complete `EntryRecord` key set (all counts/hashes/timestamps present) and byte-stable across two runs. Prefer the golden; document which was used.
4. **Markdown rejection ×7 (AC-08, R-05):** `format=markdown` on every mode → `ERROR_INVALID_PARAMS`, no JSON body, reason substring (`"markdown"`, `"format=json"`).
5. **Legacy alias (AC-07, R-08):** `format=summary` (no `detail`) → byte-identical to `detail=summary`; `format=summary`+explicit `detail` → error.
6. **Accept-and-ignore (AC-08, R-09):** `neighbors`/`path` with `detail` summary/full/absent → identical non-erroring output; `detail=bogus` → error.
7. **#913 size win (AC-06):** build a subgraph fixture large enough to be meaningful; default output byte-size well under a full-output baseline for the same query and valid parseable JSON. Assert the size *ratio*/threshold, not an absolute KB (fixture-dependent).

### Response-parsing note (harness gotcha)

`context_graph` summary/full responses are JSON blocks. If any mode appends a non-JSON trailer (as `context_correct` does, pattern #4469), use the brace-depth JSON extractor rather than a naive `json.loads` on the whole `result.text`. Assert on the parsed dict's key set for present/absent-field checks.

## Test Conventions

- Rust unit: `#[test]` / `#[tokio::test]`, Arrange/Act/Assert, name `test_{fn}_{scenario}_{expected}`.
- Integration: `def test_{tool_or_concept}_{behavior}(server)`; `server` fixture (fresh DB) is the default; use `populated_server`/`shared_server` only where accumulated state is needed for a size/volume scenario.
- Error-copy assertions: `ERROR_INVALID_PARAMS` code + **substring** only (R-13). No verbatim-sentence assertions.
- Multibyte fixtures: construct via `char::from_u32` / explicit codepoint bytes (pattern #4769), not opaque literals, so the straddle byte is unambiguous.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_search(topic:testing)` — surfaced #5509 (ADR-001 contract), #5449 (vnc-043 twin-literal byte-equality description guard, `test_graph_tool_attr_description_matches_const`), #4469 (infra-001 brace-depth JSON extractor for JSON+trailer responses), #4502/#4503/#4490 (GraphParams layout lock + `graph_read.rs` line budget), #2928 (backward-compat snapshot / static-grep gate pattern). Applied to the golden byte-equality plan, tools.rs description guard, and the harness parsing note.
- Stored: nothing novel at plan stage — patterns reused, not discovered. Stage 3c may store a golden-payload harness technique if one is built.
