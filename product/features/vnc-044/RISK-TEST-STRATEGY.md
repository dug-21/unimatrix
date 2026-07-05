# Risk-Based Test Strategy: vnc-044

> `context_graph` two-axis split: `format` (serialization: `markdown|json`) + `detail` (verbosity: `summary|full`) with a lean node projection (`NodeSummary` + `GraphSummaryProjection`).
> Grounded in ADR-001 (#5509 suite contract), ADR-002 (#5510 graph adoption), ARCHITECTURE.md, SPECIFICATION.md (FR-1..FR-12, AC-02..AC-09), and SCOPE-RISK-ASSESSMENT.md (SR-01..SR-09).
> Historical evidence: #3706, #4350 (UTF-8 byte-slice panic), #4831 (wire-enum blast radius), #3426 (formatter regression / golden test), #3337 (architecture string divergence).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `content_preview` UTF-8 flooring wrong — naive `&content[..256]` (or char-count) panics or emits invalid UTF-8 on a multibyte codepoint straddling byte 256 | High | Med | **Critical** |
| R-02 | `content_truncated` computed from *whether flooring moved the index* instead of `content.len() > 256` — false-negative on 257B ASCII that floors to exactly 256 | High | Med | **Critical** |
| R-03 | Default `full`→`summary` flip + projection applied to subgraph only; `chain`/`current`/`inverse`/`filter` missed or lose envelope metadata (`truncated`/`total_returned`/`depth_reached`) | High | Med | **Critical** |
| R-04 | `detail=full` no longer byte-for-byte identical to today — resolver threading reorders/reshapes the full arm (regression for existing full consumers) | High | Med | **Critical** |
| R-05 | `format=markdown` not rejected uniformly — reject placed in node-bearing arms only, so `neighbors`/`path` silently return JSON for `markdown` | High | Med | High |
| R-06 | Shared `ResponseFormat`/`parse_format`/`EntryRecord`/`EdgeRecord` behavior changed for non-graph callers (skip_serializing_if leak or new enum variant) — ~45-site blast radius | High | Low | High |
| R-07 | Summary field set drifts — `NodeSummary` includes an omitted field or edge projection leaks `direction`/`metadata`; absent-key assertions missing | Med | Med | High |
| R-08 | Legacy `format=summary` alias mis-resolved — conflict with explicit `detail` not rejected, or serialization not defaulted to `json` | Med | Med | Med |
| R-09 | `detail` on `neighbors`/`path` not accept-and-ignore — `validate_no_unsupported_params` rejects it, or `summary`/`full` yield different output | Med | Med | Med |
| R-10 | Empty / boundary content mishandled in projection — empty content, confidence `f64`, or tags hydration dropped in the lean shape | Med | Low | Med |
| R-11 | Lifecycle-vs-delivery status gap (SR-09) presented as answering #913 orientation — capability subgraph shows `active` for every node | Med | Med | Med (doc/expectation, not code) |
| R-12 | `256` re-literalled per call site instead of single-sourced `CONTENT_PREVIEW_BYTES`; drifts from ADR (SR-03, evidence #4975) | Low | Med | Low |
| R-13 | Error-message / field-set assertions written against ADR/architecture wording that diverges from the running string (evidence #3337) | Low | Med | Low |
| R-14 | Projection type lands in already-over-limit `graph_read_subgraph.rs` (742 lines) instead of new module; or `graph_read.rs` crosses 500 | Low | Low | Low |

## Risk-to-Scenario Mapping

### R-01: `content_preview` UTF-8 char-boundary flooring
**Severity**: High | **Likelihood**: Med | **Impact**: A stored `content` whose codepoint straddles byte 256 causes `&content[..256]` to panic — a **request-triggered DoS** (the panic aborts the graph call). A char-count implementation (`.chars().take(N)`) silently violates the *byte* cap (evidence #4350: char count cannot enforce a byte limit). Evidence #3706: this exact panic shipped before.

**Test Scenarios** (table-driven on `content_preview(&str) -> (String, bool)`, per AC-03b/FR-6):
1. Empty `""` → `("", false)`.
2. Content < 256B (ASCII) → whole content returned, `false`.
3. Content **exactly 256B** → whole content returned, `false` (no truncation at the boundary).
4. Content **257B ASCII** → 256B prefix, `true`.
5. **Multibyte straddling byte 256** — content where the codepoint spanning byte 256 is 2/3/4 bytes wide (build via `char::from_u32`/`fromCodePoint`-style, not bare literals — pattern #4769). Preview floors **below** 256 on a char boundary, result is valid UTF-8, `true`.
6. Content whose byte 256 lands exactly on a char boundary between two multibyte chars → floors to 256, valid, `true`.
7. Assert **no `…`/ellipsis** appended in any case.

**Coverage Requirement**: The shared helper in `response/verbosity.rs` uses the codebase idiom `while end > 0 && !content.is_char_boundary(end) { end -= 1; }` (not nightly `floor_char_boundary`, not `&s[..256]`, not `.chars().take()`). Every returned preview asserted valid UTF-8. Cases 1-6 all pass. A `should_panic`-free run over multibyte fixtures.

### R-02: `content_truncated` boolean edges
**Severity**: High | **Likelihood**: Med | **Impact**: `content_truncated` is the machine-readable signal to `context_get` the full node (FR-7). If it is derived from whether the flooring loop moved `end` (`end != 256`) rather than the byte-length compare, a **257B ASCII** payload (floors to exactly 256, `end==256`) reports `truncated:false` while real content was elided — the agent never learns to pull the full node.

**Test Scenarios**:
1. `content_truncated == (content.len() > CONTENT_PREVIEW_BYTES)` asserted directly, independent of char flooring — the byte-compare contract from ADR-002 §5.
2. 257B ASCII → `true` even though `end` floored to exactly 256 (the trap case).
3. Exactly-256B → `false`; 255B → `false`; empty → `false`.
4. Multibyte content > 256B whose preview floored to 254 → `true`.

**Coverage Requirement**: Truncation flag verified on **both sides** of the 256 boundary and decoupled from the flooring index. The 257B-ASCII-floors-to-256 case is non-negotiable (the specific false-negative).

### R-03: default-summary across ALL five node-bearing modes (architect OQ-3)
**Severity**: High | **Likelihood**: Med | **Impact**: #913 and AC-06 focus on `subgraph`; the projection applies to `subgraph`, `chain`, `current`, `inverse`, `filter` (FR-11). `current` returns a **single** `EntryRecord` (not `Vec`) — a different `to_summary_json` shape. `inverse`/`filter` carry `total_returned`; `chain` carries `Truncated`; `subgraph` carries `truncated`/`seed_ids`/`depth_reached`. A missing trait impl is a compile error (safe), but an impl that drops or mangles envelope metadata is a **silent** bug.

**Test Scenarios**:
1. `detail` absent (default) on each of the five node-bearing modes → lean projection (equals `detail=summary`, AC-05).
2. Per-mode metadata preservation under summary: `subgraph` retains `truncated`/`seed_ids`/`depth_reached`; `inverse`/`filter` retain `total_returned`; `chain` retains `Truncated`; `current` returns a single projected node (not an array).
3. Every projected node in every mode carries exactly the 8-field set (defers to R-07).

**Coverage Requirement**: One default-summary + one explicit-summary test per node-bearing mode (5 modes), each asserting both the projected node shape and the preserved envelope metadata. No mode may be covered by `subgraph` alone.

### R-04: `detail=full` byte-for-byte no-regression
**Severity**: High | **Likelihood**: Med | **Impact**: Existing full consumers depend on today's exact payload (AC-04/NFR-1/FR-10). Threading the resolver into each arm risks reordering keys, double-serializing, or routing full through the projection. Evidence #3426: formatter/serialization overhauls consistently underestimate ordering-regression risk — a golden-output test is required.

**Test Scenarios**:
1. Golden/byte-equality: `detail=full` output for a fixed multi-node subgraph query == captured pre-vnc-044 full payload, byte-for-byte.
2. `detail=full` full `EntryRecord` and full `EdgeRecord` (incl. `metadata`, `direction`, timestamps, hashes, counts) all still present.
3. Repeat golden comparison for `chain`, `current`, `inverse`, `filter` full output.

**Coverage Requirement**: Golden fixture captured against the pre-change binary and asserted byte-identical under `detail=full` for at least `subgraph` + one other node-bearing mode. Key order and field presence both asserted.

### R-05: `format=markdown` loud rejection, uniformly (all seven modes)
**Severity**: High | **Likelihood**: Med | **Impact**: If the reject lives in the node-bearing arms, `neighbors`/`path` (which never touch the projection) silently return JSON for `format=markdown` — a silent no-op, the exact bug vnc-044 exists to kill (D-4/FR-8/AC-08). `resolve_graph_output` runs **before** mode dispatch precisely so rejection is uniform.

**Test Scenarios**:
1. `format=markdown` on **each of all seven modes** (subgraph, chain, current, inverse, filter, neighbors, path) → `ERROR_INVALID_PARAMS`, no JSON body.
2. Error payload names the reason (no graph-markdown renderer) and points to `format=json` (SR-05) — asserted by **substring/reason presence**, not verbatim string (see R-13).
3. `format=json` and `format` absent → accepted (Json) on all modes.

**Coverage Requirement**: Rejection asserted on all seven modes, confirming resolution happens pre-dispatch. Substring assertion on the reason, not the full sentence.

### R-06: shared types unchanged for non-graph callers (SR-06/SR-07)
**Severity**: High | **Likelihood**: Low | **Impact**: `ResponseFormat`/`parse_format`/`EntryRecord`/`EdgeRecord` are suite-shared. Adding `skip_serializing_if` to `EntryRecord`/`EdgeRecord` leaks lean output into every serializer; adding a `ResponseFormat` variant triggers exhaustive-match breakage across ~45 sites (evidence #4831, NFR-2/C-3/C-4). Low likelihood because the architecture forbids it — but high impact if the guard slips.

**Test Scenarios**:
1. Existing `context_get`/`context_search`/`context_lookup`/`context_status`/mutation tests still pass with full `EntryRecord` output unchanged (regression suite green).
2. Static assertion / code review: no `skip_serializing_if` on `EntryRecord` or `EdgeRecord`; `NodeSummary` and the edge projection are distinct types / `serde_json::Value` builders.
3. `cargo test --workspace --no-run` compiles with no new exhaustive-match arms on `ResponseFormat` (blast-radius discipline, #4831).
4. A non-graph tool serializing an entry still emits `content`, hashes, timestamps, counts.

**Coverage Requirement**: Non-graph serialization regression suite green + confirmation the shared enum/structs are untouched (grep/code-review gate, not just tests).

### R-07: exact summary field set (present AND absent keys)
**Severity**: Med | **Likelihood**: Med | **Impact**: AC-03 requires the node to serialize to **exactly** `{id,title,category,tags,status,confidence,content_preview,content_truncated}` and the edge to **exactly** `{source_id,target_id,relation_type,depth}`. A leaked field (e.g. edge `direction`/`metadata`, node `content`/`content_hash`/timestamps) defeats the payload-size goal.

**Test Scenarios**:
1. Summary node JSON key set == the 8-field set — assert **present keys AND absent keys** (`content`, `content_hash`, `previous_hash`, `embedding_dim`, timestamps, `created_by`/`modified_by`, counts all absent).
2. Summary edge JSON key set == `{source_id,target_id,relation_type,depth}` — `direction` and `metadata` absent.
3. `status` field value is the lifecycle string via `status_str(entry.status)` (`active|deprecated|proposed|quarantined`), not delivery status.

**Coverage Requirement**: Absent-key assertions are mandatory, not just present-key. Both node and edge shapes covered.

### R-08: legacy `format=summary` alias + conflict
**Severity**: Med | **Likelihood**: Med | **Impact**: FR-9/AC-07. Alias must map to `detail=summary` + serialization `json`. ADR-002 §2 requires `format=summary` **with an explicit `detail`** to be `ERROR_INVALID_PARAMS` (conflict).

**Test Scenarios**:
1. `format=summary` (no `detail`) → byte-identical to `detail=summary` output; accepted, no error.
2. `format=summary` + `detail=full` → `ERROR_INVALID_PARAMS` (conflict, do-not-combine).
3. `format=summary` + `detail=summary` → `ERROR_INVALID_PARAMS` (still a conflict per resolver order) — pin the resolver's decision.

**Coverage Requirement**: Alias equivalence + conflict rejection both tested; the conflict-with-explicit-`detail` branch pinned.

### R-09: `detail` accept-and-ignore on `neighbors`/`path`
**Severity**: Med | **Likelihood**: Med | **Impact**: FR-8/AC-08 + ADR-002 §1: `detail` is a **universal** field — `validate_no_unsupported_params` must add **no** rejection arm. Risk: it is treated as unsupported on edge-only modes, or `summary`/`full` change output.

**Test Scenarios**:
1. `neighbors` and `path` with `detail=summary`, `detail=full`, and `detail` absent → all three produce **identical, non-erroring** output.
2. `detail=bogus` on `neighbors`/`path` → still `ERROR_INVALID_PARAMS` from `parse_detail` (universal parse still runs; accept-and-ignore is about effect, not validity).

**Coverage Requirement**: Identical output across `detail` values on both edge-only modes; invalid `detail` still rejected by the shared parser.

### R-10: empty/boundary content and projection fidelity
**Severity**: Med | **Likelihood**: Low | **Impact**: Empty content, tags hydration, and `confidence: f64` must survive the lean shape.

**Test Scenarios**:
1. Node with empty `content` → `content_preview: ""`, `content_truncated: false`, valid JSON.
2. Node with multiple tags → `tags` array fully preserved in the projection (fetch still hydrates tags per batch).
3. `confidence` serialized as a JSON number, unmodified.

**Coverage Requirement**: Empty-content projection and tag-preservation covered.

### R-11: lifecycle-vs-delivery status (SR-09) — documentation/expectation risk, NOT a code test
**Severity**: Med | **Likelihood**: Med | **Impact**: The projection carries lifecycle `EntryRecord.status`; a subgraph of capability entries returns `active` for **every** node and does **not** deliver the #913 orientation delivery-status (`missing|partial|proven|claimed`) tally. If copy implies otherwise, agents misread the result (AC-06 caveat, ADR-001 §7).

**Test Scenarios** (verification is by review, not assertion):
1. Tool-description review: states summary `status` is **lifecycle**, not capability delivery status, and points delivery-status needs at `context_get`/follow-up #3 (FR-12/AC-09).
2. AC-06 doc-review: the #913 reproduction result is documented as carrying lifecycle status only.
3. (Optional behavioral illustration) a capability-node subgraph asserted to return `status:"active"` for every node — evidence the gap is real, not a claim it is fixed.

**Coverage Requirement**: This is a **documentation/expectation gate**, not a functional pass/fail. Confirm the caveat appears in the tool description and AC-06 notes. Do **not** write a test that treats delivery-status absence as a defect.

### R-12: `256` single-sourced constant
**Severity**: Low | **Likelihood**: Med | **Impact**: SR-03/C-9, evidence #4975 (locked ADR value drifting downstream). If `256` is re-literalled per call site it will drift from the ADR when a follow-up tool adopts the contract.

**Test Scenarios**:
1. `content_preview` and any preview cap reference `CONTENT_PREVIEW_BYTES` from `response/verbosity.rs` — grep/code-review confirms no bare `256` literal in the graph path.

**Coverage Requirement**: Single-source confirmed by review; preview-length tests reference the constant symbolically.

### R-13: assertion strings vs running strings (evidence #3337)
**Severity**: Low | **Likelihood**: Med | **Impact**: ADR-001, ADR-002, and SPECIFICATION.md quote the `format=markdown` rejection message with slightly different wording. A tester asserting the verbatim architecture string will get failures unrelated to correctness (pattern #3337).

**Test Scenarios**:
1. Error-message tests assert `ERROR_INVALID_PARAMS` + a stable **substring** (`"markdown"`, `"format=json"`), not the full sentence.

**Coverage Requirement**: No verbatim-sentence assertions on error copy; assert error code + reason substring.

## Integration Risks

- **Resolver-before-dispatch seam** (`resolve_graph_output` at top of `handle_graph`, fixing `graph_read.rs:251`): the resolved `(Detail, GraphSerialization)` must reach **every** arm. Integration test — same query, `detail=summary` vs `detail=full`, yields different payloads (AC-02) confirming the value is actually threaded, not re-dropped.
- **Five envelope trait impls** (`GraphSummaryProjection` for `SubgraphResponse`/`ChainResult`/`CurrentResponse`/`InverseResponse`/`FilterResponse`): cross-mode consistency (Pattern #4500). Each impl independently maps node bodies and preserves its own metadata — the integration bug surface is per-envelope metadata loss (R-03).
- **`fetch_nodes_batch` untouched** (SR-01): preview is computed from the read `content`; a regression that drops `content` from `ENTRY_COLUMNS` would break preview silently. Assert preview is non-empty for non-empty content end-to-end.
- **`GraphParams` additive `detail` field** (ADR-003 layout lock): existing `GraphParams` layout test must still pass; the new `Option<String>` field is additive, no reorder/removal (AC-09/NFR-3).

## Edge Cases

- Empty `content` → `("", false)` (R-01/R-10).
- Content exactly 256B / 255B / 257B (R-02).
- Multibyte codepoint (2/3/4-byte) straddling byte 256; byte 256 exactly on a boundary (R-01).
- `current` mode single-node (non-`Vec`) projection shape (R-03).
- `detail` present on `neighbors`/`path` (R-09).
- `format=summary` + explicit `detail` conflict (R-08).
- Node with zero tags vs many tags (R-10).
- Subgraph at `max_nodes`/`truncated` cap — envelope `truncated` flag must survive the summary projection (R-03).

## Security Risks

`context_graph` accepts two untrusted string parameters (`format`, `detail`) from the MCP caller, and projects DB-resident `content` (which itself may contain adversarial or arbitrary UTF-8) into `content_preview`.

- **Untrusted input**: `format`/`detail` strings — must be rejected via `ERROR_INVALID_PARAMS` (`parse_detail`, `resolve_graph_output`), never panic on an unexpected value.
- **Primary attack surface — preview slicing (R-01)**: stored `content` whose codepoint straddles byte 256 is a **request-triggered panic (DoS)** under a naive `&content[..256]`. Because `content` is attacker-influenceable (any stored entry), this is the highest-blast-radius issue in the feature. The mandated char-boundary floor + boundary tests are the mitigation; a fuzz/property test over random multibyte content (no panic, always valid UTF-8) is recommended (cf. #4863 no-panic on untrusted input).
- **Blast radius if compromised**: bounded — the projection is read-only serialization; no injection sink (output is `serde_json`-encoded, not templated). No path traversal or deserialization of untrusted bytes. Content is already valid UTF-8 (Rust `String` from the store), so the risk is *slicing*, not *decoding*.
- **No new external surface**: `detail` is additive on an existing tool; no new endpoint, no new deserialization boundary.

## Failure Modes

| Condition | Expected behavior |
|-----------|-------------------|
| `format=markdown` (any mode) | `ERROR_INVALID_PARAMS`, names reason + `format=json`, no JSON fallback |
| `format=summary` + explicit `detail` | `ERROR_INVALID_PARAMS` (deprecated-alias conflict) |
| `format` not in `{json,markdown,summary}` | `ERROR_INVALID_PARAMS` |
| `detail` not in `{summary,full}` | `ERROR_INVALID_PARAMS` (from `parse_detail`, all modes) |
| Multibyte content straddling byte 256 | preview floors to char boundary, valid UTF-8, `content_truncated:true` — **never panics** |
| `serde_json::to_string` failure | `ERROR_INTERNAL` (existing path, unchanged) |
| `detail` on `neighbors`/`path` | accepted, ignored, output unchanged |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (full `content` still read; wire-size win, not DB win) | R-10, R-11(framing) | Not a code defect. `fetch_nodes_batch` untouched by design; NFR-4 scopes the win to payload size. Test asserts preview computed from read content; performance framing is a doc gate. |
| SR-02 (UTF-8 char-boundary truncation) | **R-01, R-02** | Fully covered — the two Critical risks. Boundary table (empty/255/256/257/straddle) + truncated-flag byte-compare. |
| SR-03 (ADR suite-drift; single-source 256/spelling/field set) | R-12, R-07, R-06 | `CONTENT_PREVIEW_BYTES` single-source (R-12); exact field-set assertions (R-07); shared-type guard (R-06). Suite-wide drift beyond `context_graph` is out of implementation scope — ADR review, not a vnc-044 test. |
| SR-04 (default flip full→summary, silent behavior change) | **R-03**, R-11 | Default-summary tested across all five node-bearing modes (AC-05); flip disclosed in tool description (R-11 doc gate). |
| SR-05 (`format=markdown` discoverability cliff) | **R-05**, R-13 | Uniform loud rejection across all seven modes with reason + `format=json` pointer; substring assertion (R-13). |
| SR-06 (`ResponseFormat`/`parse_format` shared) | **R-06** | Non-graph regression suite green; graph uses its own `resolve_graph_output`, shared enum untouched; `--no-run` compile guard (#4831). |
| SR-07 (no `skip_serializing_if` leak on `EntryRecord`/`EdgeRecord`) | **R-06**, R-07 | Distinct `NodeSummary`/edge-`Value` projection; code-review gate confirms shared structs untouched. |
| SR-08 (file-size limit) | R-14 | Projection in new `graph_read_projection.rs`; `graph_read.rs` line-count watched. File-size gate, low priority. |
| SR-09 (lifecycle vs delivery status) | **R-11** | Documentation/expectation gate — tool description + AC-06 caveat; NOT a functional test. Delivery-status promotion is named follow-up #3. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-04) | ~22 (7 preview-boundary + 4 truncated-flag + 5×2 per-mode default/summary + 2 golden byte-equality) |
| High | 3 (R-05, R-06, R-07) | ~13 (7-mode markdown reject + 4 shared-type/regression + present/absent key sets for node+edge) |
| Medium | 4 (R-08, R-09, R-10, R-11) | ~10 (alias+conflict, accept-and-ignore ×2 modes, empty/tags, doc-review gates) |
| Low | 3 (R-12, R-13, R-14) | ~4 (single-source grep, substring-assert discipline, file-size gate) |

**Non-negotiable minimum bar** (spec AC-03b + architect OQ-3 + SR-02/SR-04):
- UTF-8 preview boundary table: empty / <256 / exactly-256 / 257-ASCII / multibyte-straddle-256 — all pass, all valid UTF-8, no ellipsis (R-01).
- `content_truncated` byte-compare, both sides of 256, incl. 257B-floors-to-256 false-negative trap (R-02).
- Default-summary + explicit-summary per **each** of the five node-bearing modes with envelope-metadata preservation (R-03).
- `detail=full` golden byte-equality (R-04).
- `format=markdown` rejected on **all seven** modes (R-05).
- Present-AND-absent key-set assertions for node and edge (R-07).

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for UTF-8 truncation, wire-enum blast radius, regression/golden patterns — found #3706, #4350 (byte-slice panic / byte-cap enforcement), #4831 (enum-variant blast radius), #3426 (formatter regression golden test), #3337 (architecture string divergence), #4863/#4769 (no-panic on untrusted input, adversarial-string construction). All applied to R-01/R-02/R-04/R-06/R-13 and the Security section.
- Stored: see agent report.
