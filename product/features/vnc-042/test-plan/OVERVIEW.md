# vnc-042 Test Plan — OVERVIEW

**Feature:** `context_get` resolves superseded entries to their active terminal by default
(`follow_supersessions: Option<bool>`, handler-owned default = follow). Behavior LOCKED (#843).

**Human-flagged primary priority:** the test/CI blast radius. This plan operationalizes
RISK-TEST-STRATEGY.md so nothing surfaces at delivery (vnc-038 mode). Regression canaries stay
green with zero edits; any needed edit to a canary is a **FLAG event**, never a silent fix (#5099).

---

## 1. Test Strategy (unit / integration / feature-level)

| Layer | Where | What it proves |
|-------|-------|----------------|
| Unit (formatter) | `crates/unimatrix-server/src/mcp/response/mod.rs` (+ `entries.rs`) | `format_single_entry` byte-identity untouched; `format_single_entry_with_note` renders each `ResolutionNote` variant per format; strip-and-compare additivity |
| Unit (handler / params) | `crates/unimatrix-server/src/mcp/tools.rs` (`#[cfg(test)]`) | `GetParams` serde three-state; resolution-branch effective-id selection; dead-end fail-loud; behavioral default-on; orthogonality matrix; canonical call-site |
| Unit (visibility) | build/clippy gate | `pub(crate)` widen + re-export builds warnings-clean; existing `follow_to_current` callers unchanged |
| Integration (MCP) | `product/test/infra-001/suites/` | End-to-end through the JSON-RPC binary: default-resolves, escape hatch, dead-end flag, edges keyed on terminal, orthogonality |

Test conventions: Arrange/Act/Assert; `test_{fn}_{scenario}_{expected}`; `#[tokio::test]` for
async handler paths; deterministic (no time/order dependence).

Component test-plan files map 1:1 to the pseudocode component boundaries (IMPLEMENTATION-BRIEF §Component Map):

| Component | Test plan |
|-----------|-----------|
| `context_get` handler (resolution branch, `GetParams`, tool-desc) | `context-get-handler.md` |
| response formatter (`format_single_entry_with_note`, `ResolutionNote`) | `response-formatter.md` |
| `follow_to_current` visibility widen + re-export | `follow-to-current-reexport.md` |

---

## 2. Risk → Test Mapping (authoritative, from RISK-TEST-STRATEGY.md)

| Risk | Priority | Test(s) | Owning component plan |
|------|----------|---------|-----------------------|
| R-01 notice inside `format_single_entry` breaks byte-identity | **Critical** | TS-01 canary green (unchanged) + TS-02 ~15 shape green + strip-and-compare additivity | response-formatter |
| R-02 serde-default footgun (silent default-OFF) | **Critical** | TS-09 **behavioral** field-absent⇒resolves; field-present true/false; no quoted-scalar coercion; NFR-06 additivity green | context-get-handler |
| R-03 edges keyed on requested vs resolved id | **High** | TS-03 classification table + hopped-get asserts edges == terminal's (`effective_id`); include_edges=false skips | context-get-handler |
| R-04 dead-end returns empty/silent; store-error swallowed | **High** | TS-07 orphaned/quarantined/>50-hop/cycle/store-error ⇒ non-empty loud flag, returned id == requested | context-get-handler |
| R-05 wrong `follow_to_current` copy / build breakage | **Med** | BLD-01 warnings-clean build; BLD-02 canonical call-site grep; BLD-03 existing callers green | follow-to-current-reexport |
| R-06 `_with_note` drifts from base formatter shape | **Med** | AC-01 body-equivalence across formats; strip-and-compare | response-formatter |
| R-07 json `resolution` object leaks on clean passthrough | **High** | TS-05 clean⇒no `resolution` key (ties to TS-01); presence/absence for all 4 ADR-003 cases | response-formatter + handler |
| R-08 footer on NULL-`superseded_by` deprecated | **Med** | TS-06 ext (AC-08): `None`⇒`deprecated; no recorded successor.`; no panic, no `#null`/`#{}` | response-formatter |
| R-09 default-flip on non-code callers | **High (accept)** | BLD-04 tool-desc mentions `follow_supersessions` + default + escape hatch; behavioral non-code coverage **flagged impossible by design** | context-get-handler |
| R-10 graph-vs-get naming/default divergence | **Med (accept)** | Documented in ADR-001; review-time awareness, not gated | — |
| R-11 JS `GetParams` schema parity | **Med (CI)** | CI-01 additive-field parity — JS CI matrix (incl. Windows) only; budget one post-PR round-trip | integration §4 |
| R-12 unresolved edge targets (NG-1) | **Low (accept)** | Legible via notice; not gated | — |

**Regression guards — edits are FLAG events, must stay green:**
TS-01 byte-identity canary · TS-02 ~15 shape tests · NFR-06 param-additivity ·
`graph_queries_tests.rs` hop-cap/orphan-guard suite.

**AC → TS coverage:** AC-01→TS-04; AC-02→TS-04/TS-05; AC-03→TS-06; AC-04→TS-07; AC-05→BLD-02
(grep); AC-06→TS-09 (behavioral); AC-07→TS-08; AC-08→TS-06 ext.

---

## 3. Cross-Component Test Dependencies

- **`id → effective_id` threading (R-03, highest-probability integration defect):** the resolved
  id must flow to **both** `entry_store.get(effective_id)` **and** `build_edges_view(store,
  effective_id)`. This is a handler-owned assertion but is only observable through the formatter
  output (returned entry id) + edge list. The hopped-get integration test (§4) is the end-to-end
  proof that the partial-swap defect (terminal content + requested-id edges) is absent.
- **Formatter route selection:** handler picks `CleanPassthrough → format_single_entry` vs
  `else → format_single_entry_with_note`. R-01/R-07 (formatter plan) depend on the handler never
  routing a clean passthrough through the note wrapper. Covered by TS-05 (clean⇒no note/no key).
- **Dead-end variant plumbing:** handler constructs `DeadEnd{requested:id}` (ADR-002); formatter
  renders it (ADR-003). TS-07 asserts the handler-side returned-id AND the formatter-side flag
  text/json — one test spans both boundaries.

---

## 4. Integration Harness Plan (infra-001)

Harness exercises the compiled `unimatrix-server` binary over MCP JSON-RPC. Read
`product/test/infra-001/USAGE-PROTOCOL.md` before Stage 3c.

### 4.1 Suites to run (per suite-selection table)

Feature touches: **server tool logic**, **store/retrieval behavior**, **schema/storage-adjacent
supersession reads**. Selection:

| Suite | Why | Gate |
|-------|-----|------|
| `smoke` (`-m smoke`) | Mandatory minimum gate — any change | **MUST pass** |
| `test_tools.py` | `context_get` is a tool; new param exercised | run full |
| `test_protocol.py` | tool-discovery / schema surfaces the new param | run full |
| `test_lifecycle.py` | correction chains → the exact A→B→terminal supersession flow (`test_multi_step_correction_chain:316`, `test_correction_chain_integrity:45`) | run full |
| `test_get_edges.py` | edges-of-queried-id; resolved-terminal edge keying (R-03) | run full |
| `test_edge_cases.py` | empty-DB / boundary get behavior | run full |

`test_confidence.py`, `test_contradiction.py`, `test_security.py`, `test_volume.py` are not
feature-touched (resolution changes *which* already-authorized entry returns, adds no data reach —
RISK §Security "no new attack surface"); run smoke-level only unless a failure implicates them.

### 4.2 Integration scenarios to validate (through MCP)

Anchor on existing correction-chain fixtures in `test_lifecycle.py` (uses `context_correct` to
build A→B chains, then `context_get`).

1. **Default resolves (AC-01/AC-06):** correct A→B, then `context_get(A)` (param omitted) ⇒ returned
   entry id == B, body == B's content, `↻` notice / json `resolution.status=="followed"`.
2. **Clean passthrough (AC-02/R-07):** `context_get(B)` ⇒ no notice; `format="json"` ⇒ **no
   `resolution` key** (byte-identity preserved end-to-end).
3. **Escape hatch (AC-03):** `context_get(A, follow_supersessions=False)` ⇒ id==A, deprecated
   content, footer `deprecated; superseded by #B …`.
4. **Dead-end fail-loud (AC-04):** build a chain dead-ending on a deprecated/quarantined entry with
   no active successor ⇒ non-empty result, returned id == requested, loud `⚠` flag /
   `resolution.status=="no_active_successor"`. Assert **not** an MCP error and **not** empty.
5. **Orthogonality (AC-07):** matrix `format ∈ {null,markdown,json}` × `include_edges ∈
   {omitted,true,false}` on the hopped get ⇒ resolves to B in every cell; hopped get with
   `include_edges` ⇒ edge list is **B's** (keyed on `effective_id`).

### 4.3 New integration tests to add (Stage 3c)

New behavior visible only through MCP ⇒ add tests; place per convention (extend, never scaffold):

| Test | File | Fixture | Covers |
|------|------|---------|--------|
| `test_get_default_resolves_deprecated_to_terminal` | `test_lifecycle.py` | `server` | AC-01/AC-06 scenario 1 |
| `test_get_clean_passthrough_no_resolution_key` | `test_lifecycle.py` | `server` | AC-02/R-07 scenario 2 |
| `test_get_follow_false_returns_as_stored_with_footer` | `test_lifecycle.py` | `server` | AC-03 scenario 3 |
| `test_get_deadend_returns_requested_id_loud_flag` | `test_lifecycle.py` | `admin_server` (quarantine) | AC-04 scenario 4 |
| `test_get_resolved_edges_keyed_on_terminal` | `test_get_edges.py` | `server` | R-03 / AC-07 edge keying |
| `test_get_follow_supersessions_orthogonal_matrix` | `test_tools.py` | `server` | AC-07 matrix |

**Required harness client extension (tracked, not a surprise):** `HarnessClient.context_get`
(`harness/client.py:496-518`) and the UDS mirror (`harness/uds_client.py:379`) must gain a
`follow_supersessions: bool | None = None` kwarg mirroring the existing `include_edges` opt-out
(absent ⇒ omitted from args ⇒ server default-on). This is additive; add it before writing 4.3 tests.

### 4.4 When NOT to add integration tests
- Byte-identity / strip-and-compare / serde three-state ⇒ unit tests (formatter/handler); not
  MCP-visible in a way the harness asserts.
- Hop-cap / orphan-guard chain correctness ⇒ `graph_queries_tests.rs` stays the authority; the
  harness adds only the `context_get`-level >50-hop exercise (scenario 4 variant).
- Store-layer read-back-after-deprecate ⇒ **false positives, EXCLUDED** (#5383). Do not add or migrate.

---

## 5. CI Notes (blast radius — Linux-only local gates)

- Local gates 3a/3b/3c are **Linux-only**; the **JS CI matrix (incl. Windows) is the cross-platform
  gate**. Rust validation = protocol cargo-test gates + release workflows **by design** — a missing
  Rust CI job is **not** a gap.
- **CI-01 (R-11):** if the JS edge client mirrors a `GetParams` schema, the additive
  `follow_supersessions` field may need parity there; a mismatch surfaces only in the JS CI matrix.
  **Budget one post-PR CI round-trip.** Not a local-gate failure.
- **Fixture guard (#5099 / vnc-038 mode):** no fixtures/goldens are known to encode `context_get`
  response content (verified SR-02). IF one encoding the OLD default is found at delivery, migrating
  it is development work to be **FLAGGED, not silently narrowed**.

---

## 6. Open Questions
- OQ-3 sub-toggle: ADR-003 pins json as a structured `resolution` object (recommended). If the human
  later prefers flat-`"note"` parity, the json assertions in the formatter plan flip shape — flagged,
  not blocking.
- R-09 non-code-consumer behavioral coverage is **impossible by design** (memory files, edges, prior
  sessions are outside any harness). Testable proxy only (BLD-04 tool-desc). **Flagged for human.**

## Knowledge Stewardship
- Queried: `context_briefing` + `context_search` — surfaced #5388 (ADR-001 vnc-042 default divergence),
  #4781 (Stage-3c pre-existing-failure xfail procedure), #5383 (blast-radius partitioning; store-layer
  read-backs are false positives — applied to §4.4 exclusion), #3789 (mandatory MCP-dispatch integration
  test — applied to §4.2). No novel test-infra pattern to store yet (harness `context_get` kwarg is an
  additive mirror of the existing `include_edges` pattern, #5383 governs the exclusion); revisit at Stage 3c.
