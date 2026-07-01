# vnc-042 Acceptance Criteria Map

`context_get` resolves superseded entries to current by default. Behavior LOCKED (GH #843).
AC-01..AC-07 verbatim from #843; AC-08 is spec-derived edge-case hardening (R-08), does not
alter the locked set.

## Acceptance Criteria

| AC-ID | Description | Source | Verification Method | Verification Detail | Status |
|-------|-------------|--------|---------------------|---------------------|--------|
| AC-01 | Deprecated `id` returns the full content of the active terminal, identical in shape to a direct get of the terminal | #843 / FR-02 | test | Handler test: correct A→B (B active). `context_get(A)` (default) and `context_get(B)` produce structurally identical entry payloads; assert returned entry id == B and body == B's stored content. (TS-04) | PENDING |
| AC-02 | Hop → one-line `↻ Requested #X (deprecated) → returning current version #Y` notice; no hop → no notice (clean passthrough) | #843 / FR-04, FR-05 | test | (hop) `context_get(A)` contains exact notice string X=A,Y=B; (no-hop) `context_get(B)` contains no notice substring. json: `resolution` presence/absence per ADR-003. (TS-04, TS-05) | PENDING |
| AC-03 | `follow_supersessions=false` returns entry exactly as stored for any status, with deprecated-footer pointer when deprecated | #843 / FR-06, FR-07 | test | `context_get(A, false)` → A's stored content (id==A, deprecated) + footer `deprecated; superseded by #B (omit follow_supersessions to follow).`. `context_get(B, false)` → B verbatim, no footer. (TS-06) | PENDING |
| AC-04 | Chain terminating on a non-active entry (orphaned/quarantined/>50-hop) returns a result with a loud non-active flag — never empty, never silent; returned id == originally-requested id | #843 / FR-08 / ADR-002 | test | Construct deprecated entry dead-ending on orphaned/quarantined non-active entry, and separately a >50-hop chain. `context_get(id)` (default) → non-empty result, entry id == requested id, loud `⚠ … no active successor` flag present. json `{"status":"no_active_successor","requested_id":X}`. (TS-07) | PENDING |
| AC-05 | Resolution reuses `follow_to_current` (hop cap + fallback); no new chain-walk added | #843 / FR-03 / NFR-03 | grep | Code review + call-site assertion: handler calls `crate::mcp::graph_read::follow_to_current`; no new recursive CTE or in-memory walk. `grep` for absence of new chain traversal in `tools.rs`; existing `graph_queries_tests.rs` stays the chain-correctness authority. | PENDING |
| AC-06 | Resolution is on by default when the parameter is omitted (the contract change is the new default) | #843 / FR-01 / NFR-01 / ADR-001 | test | **Behavioral** (authoritative): JSON payload OMITTING `follow_supersessions` → `context_get(A_deprecated)` resolves to terminal B (== AC-01 path). Guards ADR-001 handler-owned-default (a bare `#[serde(default)] bool` would default-OFF and pass a serde round-trip while failing here). (TS-09) | PENDING |
| AC-07 | `follow_supersessions` composes correctly with `format` and `include_edges` (orthogonal) | #843 / FR-10, FR-11 | test | Matrix: deprecated A→B across `format ∈ {null,markdown,json}` × `include_edges ∈ {omitted,true,false}`; assert resolution to B in every cell, `format` renders B, edges keyed on `effective_id` (ADR-D1), byte-identity canary unaffected for `format=null`. (TS-08) | PENDING |
| AC-08 | `follow_supersessions=false` on an orphaned/quarantined deprecated entry (`superseded_by IS NULL`) returns as-stored with a well-formed-or-absent footer — no panic, no malformed `#{}` | spec / R-08 / FR-07 / ADR-003 §4 | test | Construct deprecated entry with `superseded_by = NULL`. `context_get(id, false)` → entry verbatim; assert no panic, no `#{}`/`#null` substring, footer == `deprecated; no recorded successor.` (json `superseded_by: null`). (TS-06 ext) | PENDING |

## Test / CI Blast Radius (SR-02 — human-flagged priority, tracked rows)

Enumerated now so fixture/test migration is NOT a delivery-time surprise (cf. vnc-038).
Store-layer read-back-after-deprecate tests are FALSE POSITIVES (call `store.get()`, stay
as-stored) — excluded per #5383, do NOT migrate. Delivery agents MUST FLAG (not silently
narrow/edit) any adjacent breakage (#5099).

### Regression guards — MUST stay green, edits are FLAG events

| TS-ID | Cluster | Location | Verification Method | Verification Detail | Status |
|-------|---------|----------|---------------------|---------------------|--------|
| TS-01 | Byte-identity canary | `test_none_json_byte_identical_to_base_object`, `response/mod.rs:~367` (ADR-003, ref `tools.rs:999`) | test | `cargo test` — MUST pass unchanged. Clean passthrough (`format=null`) yields byte-for-byte identical `CallToolResult`. Any edit to this test = FLAG event (R-01/R-07, NFR-05). | PENDING |
| TS-02 | `format_single_entry` shape tests (~15 sites) | `response/mod.rs:296-469` | test | `cargo test` — all ~15 stay green; base formatter untouched. Breakage = signal notice was mis-placed inside `format_single_entry` (FR-09/C-7 violated). | PENDING |
| TS-03 | `include_edges` contract + param-additivity (~18 tests) | `get_edges_tests.rs` (`tools.rs:5630-5690`), incl. `test_get_params_no_existing_field_removed_or_retyped` (NFR-06) | test | Classify each of ~18 into (a) unaffected (no hop, `effective_id==id`, stays green) vs (b) needs resolved-case assertion (edges keyed on terminal per ADR-003). Classification is tracked development work, NOT a delivery surprise (#5099). Additivity test stays green. | PENDING |

### New coverage required by the change

| TS-ID | Cluster | Covers | Verification Method | Verification Detail | Status |
|-------|---------|--------|---------------------|---------------------|--------|
| TS-04 | Default resolves deprecated → terminal | AC-01 / AC-06 | test | New handler test; default path hop A→B. | PENDING |
| TS-05 | Clean passthrough, no notice | AC-02 (no-hop) | test | `context_get(B)` (active terminal) → no notice; json → no `resolution` key (ties to TS-01). | PENDING |
| TS-06 | `follow_supersessions=false` exact-as-stored + footer (incl. NULL-`superseded_by`) | AC-03 / AC-08 | test | As-stored content + footer; orphaned/quarantined NULL case well-formed/absent, no panic. | PENDING |
| TS-07 | Dead-end fail-loud flag | AC-04 | test | Orphaned, quarantined, >50-hop, cycle/self-loop, and store-error-collapse → non-empty flagged result, returned id == requested id. Exercise >50-hop cap THROUGH `context_get`. | PENDING |
| TS-08 | Orthogonality matrix | AC-07 | test | `format` × `include_edges` matrix; `resolution`-key presence/absence for all four ADR-003 cases. | PENDING |
| TS-09 | Backward-compat, handler-owned default | AC-06 / NFR-01 / NFR-02 | test | **Behavioral** field-absent → resolves (single highest-value test); field-present `true`/`false`; no quoted-scalar coercion (plain `Option<bool>`, #3728). | PENDING |

### Build / integration guards

| ID | Check | Verification Method | Verification Detail | Status |
|----|-------|---------------------|---------------------|--------|
| BLD-01 | Visibility widen builds clean under warnings-as-errors | shell | `cargo build` + `cargo clippy -- -D warnings` green after `pub(super)→pub(crate)` widen + re-export; no `dead_code`/unused warnings (R-05). | PENDING |
| BLD-02 | Canonical call-site | grep | Handler invokes `crate::mcp::graph_read::follow_to_current`, NOT the `graph_read_supersession.rs:122` duplicate (R-05, Pattern #4436). | PENDING |
| BLD-03 | Existing supersession-walk tests unchanged | test | `graph_queries_tests.rs` hop-cap/orphan-guard suite + neighbors/subgraph callers stay green (C-3, #4538). | PENDING |
| BLD-04 | Tool-description updated | grep | `context_get` description strings (`tools.rs:947-948`) mention `follow_supersessions` + the new default + escape hatch (C-5, NFR-08, #4303). | PENDING |
| CI-01 | JS edge-client `GetParams` schema parity | manual | JS CI matrix (incl. Windows) is the cross-platform gate; additive `follow_supersessions` field may need JS parity — surfaces only in CI, not Linux-only local gates. Budget one post-PR CI round-trip (R-11). | PENDING |

**Fixture note:** no fixtures/goldens are known to encode `context_get` response content
(verified SR-02). IF one encoding the OLD default is found at delivery, migrating it is
development work to be FLAGGED, not silently narrowed (#5099, vnc-038 mode).
