# Gate 3b Report: vnc-044

> Gate: 3b (Code Review)
> Date: 2026-07-05
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | Code matches all four component pseudocode files; only deviation is a cleaner generic `serialize_detail<T>` helper (DRY-equivalent to the pseudocode's per-arm inline match). Full arm serializes the original struct directly. |
| 2. Architecture compliance | PASS | ADR-001 + ADR-002 followed: resolver runs pre-dispatch, distinct projection module, shared primitives single-sourced, `GraphParams` additive, `EntryRecord`/`EdgeRecord`/`ResponseFormat` untouched. |
| 3. Interface implementation | PASS | `parse_detail`, `content_preview`, `node_summary`, `resolve_graph_output`, `GraphSummaryProjection` signatures all as specified. |
| 4. Test case alignment | PASS | R-01..R-13 covered at unit level; test plans mapped. Golden byte-equality end-to-end and all-seven-mode live checks are Stage 3c. |
| 5. Code quality | PASS (2 WARN) | New files under 500 lines; no stubs; no `.unwrap()`/`.expect()` in new prod code; builds + clippy clean. WARNs = pre-existing over-limit files + deferred cosmetic doc comment (both out of scope). |
| 6. Security | PASS | `content_preview` char-boundary floor never panics (fuzz-tested); untrusted `format`/`detail` rejected via `ERROR_INVALID_PARAMS`; read-only serialization, no injection/traversal/deser surface; no secrets; no dependency changes. |
| 7. Knowledge stewardship | PASS | All three impl agents (verbosity, tools, projection-seam) have `## Knowledge Stewardship` blocks with `Queried:` + `Stored:`/declined-with-reason. |

**Build:** `cargo build -p unimatrix-server` clean.
**Tests:** `cargo test -p unimatrix-server --lib` → 4482 passed, 0 failed, 1 ignored. 96 vnc-044-related tests green.
**Clippy:** `cargo clippy -p unimatrix-server --lib` clean.

## Critical Gate Verification (spawn-prompt "VERIFY IN PARTICULAR")

- **R-01 (UTF-8 char-boundary floor):** `verbosity.rs:60-69` uses the mandated `while end > 0 && !content.is_char_boundary(end) { end -= 1; }` loop — NOT `&s[..256]`, NOT `floor_char_boundary`, NOT `.chars().take()`. Boundary table tested: empty / <256 / 255 / exactly-256 / 257-ASCII / 2-3-4-byte straddle / exact-boundary / all-multibyte, plus a deterministic fuzz over pad 250..=260 and mixed adversarial content asserting no panic + valid UTF-8 + genuine prefix. PASS.
- **R-02 (`content_truncated` decoupled from flooring index):** early-return `false` when `content.len() <= CAP`, unconditional `true` otherwise — flag is the byte compare, not `end != 256`. `test_content_truncated_257_ascii_true` + byte-compare invariant across [0,1,255,256,257,300,1000]. PASS.
- **R-03 (all five node-bearing modes):** `to_summary_json` impl for `SubgraphResponse`/`ChainResult`/`CurrentResponse`/`InverseResponse`/`FilterResponse`; per-envelope metadata-preservation tests (subgraph keeps truncated/seed_ids/depth_reached; chain keeps `{forward,backward}`; current = single object not array; inverse/filter keep total_returned). Default→Summary resolution pinned. PASS.
- **R-04 (`detail=full` byte-for-byte):** `serialize_detail` Full arm calls `serde_json::to_string(result)` on the ORIGINAL typed envelope — never routes through the projection. `test_full_arm_byte_identical_to_direct_to_string` + `test_full_arm_serializes_raw_result` (content/content_hash present). PASS.
- **R-05 (`format=markdown` rejected pre-dispatch):** `resolve_graph_output` runs at the top of `handle_graph` before mode dispatch, so rejection is uniform across all seven modes. `test_resolve_markdown_rejected_substring` across detail=None/summary/full asserts `"markdown"` + `"format=json"` substrings. (Full seven-mode live assertion is Stage 3c.) PASS.
- **R-07 (exact present/absent key sets):** node exact 8-key set + absent-key sweep (content, hashes, timestamps, counts, etc.); edge exact 4-key set + `direction`/`metadata` absent. PASS.
- **C-1:** `detail: Option<String>` appended at end of `GraphParams`; no field removed/retyped/reordered. Additive-deserialize test. PASS.
- **C-3:** no `skip_serializing_if` added (grep = 0); `unimatrix-store/src/schema.rs` untouched (empty diff). PASS.
- **C-4:** `response/mod.rs` diff adds only `pub mod verbosity;`; shared `ResponseFormat`/`parse_format` behavior unchanged. PASS.
- **C-9:** `256` appears only as the `CONTENT_PREVIEW_BYTES` const definition + doc comments — no bare literal in graph-path logic. PASS.

## Detailed Findings

### Check 1 — Pseudocode fidelity
**Status:** PASS. All four components match. The one intentional refinement: pseudocode showed an inline `match detail { Full => …, Summary => … }` duplicated per arm; the implementation factors this into `fn serialize_detail<T: Serialize + GraphSummaryProjection>(...)` (graph_read.rs:312-321), keeping the Full/Summary semantics byte-identical while removing five copies. Documented in a doc comment. This is a DRY improvement, not a departure from behavior.

### Check 2 — Architecture compliance
**Status:** PASS. `resolve_graph_output` is called as Step 0 of `handle_graph` before `validate_no_unsupported_params` and dispatch (ADR-002 §2). Projection lives in its own `graph_read_projection.rs` `#[path]` child module (ADR-002 §3, C-7). Shared primitives (`Detail`, `parse_detail`, `CONTENT_PREVIEW_BYTES`, `content_preview`) live once in `response/verbosity.rs`, imported by full path (ADR-001 SR-03, C-9). `fetch_nodes_batch`/`graph_read_subgraph.rs` untouched (SR-01/SR-08).

### Check 3 — Interface implementation
**Status:** PASS. Signatures match the brief. `parse_detail` returns `ServerError` (tool-agnostic), adapted to `ErrorData` by the resolver via `.map_err(ErrorData::from)`. `edge_summary` builds a distinct `serde_json::Value` (EdgeRecord unmutated).

### Check 4 — Test case alignment
**Status:** PASS. verbosity tests → R-01/R-02/R-10/parse_detail; projection tests → R-03/R-07/R-10; vnc044 seam tests → R-04/R-05/R-08/R-09 (resolver decision table)/C-1; tools tests → R-11/R-13 doc gates + twin-literal guard. Coverage is unit-level, appropriate for Gate 3b; golden byte-equality against the pre-change binary and full seven-mode live rejection are the Stage 3c tester's responsibility per RISK-TEST-STRATEGY.

### Check 5 — Code quality
**Status:** PASS with 2 non-blocking WARNs.
- New/modified module line counts: verbosity.rs 371, graph_read_projection.rs 430, graph_read.rs 469, graph_read_tests_vnc044.rs 258 — all under 500 (C-7 honored).
- No `todo!`/`unimplemented!`/`TODO`/`FIXME` in the diff. The `unreachable!` arms in `handle_graph` are pre-existing exhaustiveness guards with documented rationale, not stubs.
- No `.unwrap()`/`.expect()` in new production code (test modules only).
- **WARN (pre-existing debt, out of scope):** `tools.rs` (13369 lines) and `graph_read_subgraph.rs` (742 lines) exceed the 500-line limit. Both pre-date this feature; the brief (NOT-in-scope #8) and ADR-002 explicitly carve out `graph_read_subgraph.rs` as flagged-not-fixed debt. `tools.rs` is the long-standing tool-registry file (twin-literal description edit only). Neither is a vnc-044 regression; flagged for a future cleanup feature.
- **WARN (cosmetic, deferred by design):** the brief's comment-only doc note in `graph_read_validation.rs` ("`detail` is universal, no rejection arm") was deliberately deferred by the Wave 2 agent. Behavior is correct and test-pinned (`test_detail_not_rejected_on_neighbors`/`_on_path` pass). Not a behavioral gap; a one-line maintainer hint. Non-blocking.

### Check 6 — Security
**Status:** PASS. The primary attack surface (R-01 preview slicing on attacker-influenceable `content`) is mitigated by the total, panic-free `content_preview` with an explicit fuzz test. Both untrusted string params (`format`, `detail`) are validated and rejected with `ERROR_INVALID_PARAMS` before dispatch; no unexpected value panics. Output is `serde_json`-encoded (no template injection), read-only, no path/command surface, no new deserialization boundary. `cargo audit` N/A this feature — no `Cargo.toml`/`Cargo.lock` changes (dependency surface unchanged).

### Check 7 — Knowledge stewardship
**Status:** PASS. verbosity agent — Queried (context_search, no applicable primitive), Stored: declined with reason (gotchas already in ADR-001 §6). tools agent — Queried (context_briefing), Stored: declined with reason (twin-literal pattern already #5457/#5449/#869). projection-seam agent — Queried (context_briefing), Stored: entry #5520 (rustfmt single-file edition-2024 trap). All blocks present, all with Queried + Stored/declined-reason.

## Rework Required
None.
