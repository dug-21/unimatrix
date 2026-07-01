# Risk-Based Test Strategy: vnc-042

**Feature:** `context_get` resolves superseded entries to their current version by default (`follow_supersessions: bool = true`; `false` = as-stored). Behavior LOCKED (GH #843).
**Inputs:** SCOPE.md · ARCHITECTURE.md · ADR-001/002/003 · SPECIFICATION.md (FR-01..FR-14, TS-01..TS-09) · SCOPE-RISK-ASSESSMENT.md (SR-01..SR-07).
**Primary emphasis (human directive):** the test/CI blast radius is the top concern. This document is the **authoritative coverage plan** that prevents a vnc-038-style CI-red-on-stale-fixture surprise. Historical basis: Unimatrix #5383 (blast-radius partitioning — store-layer read-back tests are false positives), #5099 (enumerate full blast radius incl. fixtures; FLAG don't silently narrow), #3774/#3817 (serde-default dual-site silent failures), #4303 (tool descriptions must not lie).

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Resolution notice injected **inside** `format_single_entry` — breaks byte-identity canary + ~15 shape tests | High | Med | **Critical** |
| R-02 | **serde-default footgun:** `#[serde(default)]` on a plain `bool` yields Rust `Default` = `false`, silently inverting AC-06 to default-OFF | High | Med | **Critical** |
| R-03 | `include_edges` edge list keyed on wrong id (requested vs resolved terminal) — terminal content + wrong entry's edges; ~18 `get_edges_tests.rs` migration surprise | High | Med | **High** |
| R-04 | Dead-end (`follow_to_current → None`) returns empty/silent instead of loud flag; store-error collapses into `None` and is swallowed | High | Low | **High** |
| R-05 | `pub(super)`→`pub(crate)` visibility widen + re-export: wrong `follow_to_current` copy called (dup at `graph_read_supersession.rs:122`) or build/warning breakage | Med | Low | **Med** |
| R-06 | `format_single_entry_with_note` drifts from `format_single_entry` internals — shape divergence on the resolved path | Med | Med | **Med** |
| R-07 | JSON `resolution` object emitted on **clean passthrough** — breaks `format=null`/json byte-identity | High | Low | **High** |
| R-08 | Escape-hatch footer on an **orphaned/quarantined deprecated** entry whose `superseded_by IS NULL` — footer has no target to name | Med | Med | **Med** |
| R-09 | Default-contract flip silently resolves durable-id callers expecting as-stored content (non-code consumers, untestable) | High | Med | **High (accept)** |
| R-10 | Graph-vs-get naming/default divergence (`resolve_supersessions=false` vs `follow_supersessions=true`) reviewer footgun | Low | High | **Med** |
| R-11 | JS edge-client `GetParams` schema parity / transport framing — surface only the CI matrix (incl. Windows) catches, not Linux-only local gates | Low | Med | **Med (CI)** |
| R-12 | Mixed-resolution response: terminal content + terminal edge *list* but unresolved edge *targets* (old id+title) — NG-1 accepted asymmetry | Low | High | **Low (accept)** |

Priority = Severity × Likelihood; low-severity/high-likelihood accepted risks (R-09/R-10/R-12) are documented not gated.

---

## Risk-to-Scenario Mapping

### R-01: Notice injected inside `format_single_entry` (byte-identity regression)
**Severity:** High · **Likelihood:** Med · **Impact:** The byte-identity canary and ~15 shape tests go red; worse, if "fixed" by editing the golden, downstream json consumers silently break. This is the exact SR-04/SR-02a failure and the vnc-038 pattern.

**Regression guard (MUST stay green, unchanged):**
1. **TS-01** `test_none_json_byte_identical_to_base_object` (`response/mod.rs:~367`) — clean passthrough (`format=null`) must yield byte-for-byte identical `CallToolResult`. Explicit canary; any edit to this test is a **FLAG event**, not a fix (#5099).
2. **TS-02** ~15 `format_single_entry` shape tests (`response/mod.rs:296-469`) — the base formatter is untouched; breakage here is the signal FR-09/C-7 was violated (notice mis-placed).

**New scenarios:**
3. Clean passthrough across `format ∈ {null, markdown, json}` produces no notice and matches the base-formatter bytes (delegates to `format_single_entry`, ADR-003 route `mode == CleanPassthrough`).
4. `format_single_entry_with_note` output, with the note region stripped, equals the base `format_single_entry` output for the same entry (proves notice is purely additive/outside).

**Coverage Requirement:** TS-01 + TS-02 green with zero edits; the notice appears only via `format_single_entry_with_note`; canary passes under the resolved (`Followed`) path only because that path is a *different* code route, never mutating the clean route.

### R-02: serde-default footgun — silent default-OFF
**Severity:** High · **Likelihood:** Med · **Impact:** AC-06 is the entire contract change. `#[serde(default)]` on a bare `bool` resolves via `bool::default() == false` — i.e. **escape-hatch (as-stored) by default**, the opposite of the locked behavior — and it fails silently: the field deserializes, no error, wrong branch. This is Unimatrix #3774/#3817 (default lives in two independent sites; serde-default and behavioral-default must agree).

**Design guard:** ADR-001 specifies `Option<bool>` where `None | Some(true) ⇒ follow` and the handler owns the default. This is the safe shape — the default lives in **handler branch logic**, not in a `bool`'s `Default`. **FLAG if delivery implements a plain `bool` with bare `#[serde(default)]`** (spec §3 FR-01 phrases it as `bool default true`, which bare `#[serde(default)]` cannot deliver — it needs `Option<bool>` per ADR-001, or an explicit `#[serde(default = "…true")]`). ADR-001 (the ruling) governs; FR-01's prose does not override the shape.

**Scenarios (TS-09 / NFR-01 / NFR-02):**
1. JSON payload **omitting** `follow_supersessions` → behavioral assert: `context_get(A_deprecated)` **resolves to terminal B** (not as-stored A). Assert the *behavior*, not just the deserialized field value — a field-value-only test would pass even with the footgun if the enum encodes None correctly but the branch is inverted.
2. Field present `true` → follow; field present `false` → as-stored. Both explicit.
3. NFR-02: field as quoted scalar must not silently coerce — keep plain `Option<bool>`, never `deserialize_*_or_string` (#3728).
4. NFR-06 `test_get_params_no_existing_field_removed_or_retyped` stays green (additive-only).

**Coverage Requirement:** at least one **behavioral** default-on test (field absent → resolves), not merely a serde-value round-trip. This is the single highest-value test in the feature.

### R-03: `include_edges` edge list keyed on the wrong id
**Severity:** High · **Likelihood:** Med · **Impact:** ADR-003 rules edges rebuild on the **resolved-terminal** id (`build_edges_view(store, effective_id)`), changing the pre-vnc-042 call which used the requested `id` (`tools.rs:991`). If delivery misses the `id → effective_id` swap, a resolved get returns terminal content with the requested (deprecated) entry's edges — a coherence bug that no existing test catches because today `id == effective_id` always. Conversely, ~18 `get_edges_tests.rs` assertions assume edges-of-queried-id; under resolution the "queried id" for a hop is now the terminal.

**Scenarios (TS-03 / TS-08 / SR-03):**
1. **TS-03 review pass:** classify each of the ~18 `get_edges_tests.rs` (`tools.rs:5630-5690`) into (a) unaffected (non-deprecated id → no hop → `effective_id == id`, stays green) and (b) requires a resolved-case assertion. This classification is **development work tracked here, not a delivery surprise** (#5099).
2. Resolved get `context_get(A_deprecated, include_edges=true)` where B is terminal → assert returned edges are **B's** edges (keyed on `effective_id`), and returned entry id == B.
3. `include_edges=false` on a resolved get → no `edges` key (edge assembly skipped), resolution still occurs.
4. Dead-end / as-stored paths → `effective_id == requested id`, edges keyed on requested id (ADR-002/ADR-003).

**Coverage Requirement:** an explicit assertion that a **hopped** get returns the terminal's edge list; the TS-03 classification table exists before delivery closes.

### R-04: Dead-end returns empty/silent (fail-loud violation)
**Severity:** High · **Likelihood:** Low · **Impact:** AC-04 / vision principle #5. `follow_to_current` returns `None` on orphaned-deprecated terminal, quarantined terminal, >50 hops, **and internal store error** (ADR-002) — all collapse to one path. A silent-empty return violates the fail-loud contract; a store error hidden as an empty result is a false-green (#4876 — verify error propagation empirically).

**Scenarios (TS-07 / FR-08):**
1. Chain dead-ending on an **orphaned deprecated** (`superseded_by IS NULL`, `status != Active`) → non-empty result, loud `⚠ … no active successor` flag, returned id == **originally requested** id (OQ-2 rec (a) / ADR-002).
2. Chain dead-ending on a **quarantined** terminal → same loud flag.
3. **>50-hop** chain → `None` → dead-end flag (NFR-04, cap preserved, C-3).
4. **Cycle / self-loop** in `superseded_by` → 50-hop cap trips → dead-end flag (never infinite loop).
5. JSON: `{"status":"no_active_successor","requested_id":X}` present; result non-empty.

**Coverage Requirement:** every `None` sub-case yields a non-empty flagged result; a >50-hop test exercises the cap **through `context_get`** (not only the `graph_queries_tests.rs` unit).

### R-05: Visibility widening + duplicate-copy hazard
**Severity:** Med · **Likelihood:** Low · **Impact:** `follow_to_current` is `pub(super)` and duplicated (canonical `graph_read_neighbors.rs:36`; second copy `graph_read_supersession.rs:122`). Widening to `pub(crate)` + re-export from `graph_read.rs` risks (a) build failure if the re-export path is wrong, (b) `dead_code`/unused warnings that fail a `-D warnings` gate, (c) the handler calling the **non-canonical** copy.

**Scenarios:**
1. Build/clippy gate green after the widen (Linux local gate covers this).
2. Call-site assertion: handler invokes the canonical `crate::mcp::graph_read::follow_to_current` (Pattern #4436 fully-qualified path), not the `graph_read_supersession.rs:122` copy.
3. No behavioral change to existing `follow_to_current` callers (neighbors/subgraph) — their tests stay green.

**Coverage Requirement:** clean build with warnings-as-errors; existing supersession-walk tests unchanged and green.

### R-06: Formatter drift between base and `_with_note` variants
**Severity:** Med · **Likelihood:** Med · **Impact:** Two formatter entry points (`format_single_entry`, `format_single_entry_with_note`) that must render identical entry bodies, differing only by the note. Drift produces a resolved response whose body differs from a direct get of the terminal — violating AC-01 ("identical in shape").

**Scenarios:**
1. AC-01 shape equivalence: body of `context_get(A)` (hopped to B) == body of `context_get(B)` (clean), modulo the resolution note (assert same entry id, same fields, same content).
2. `_with_note` delegates to the shared rendering internals (per ADR-003), verified by the R-01 scenario 4 strip-and-compare.

**Coverage Requirement:** AC-01 body-equivalence test across `format ∈ {null, markdown, json}`.

### R-07: JSON `resolution` object leaking onto clean passthrough
**Severity:** High · **Likelihood:** Low · **Impact:** ADR-003 requires the `resolution` key be **absent** on clean passthrough to preserve json byte-identity. If the handler always attaches the object (even an empty/`clean` variant), the common active-entry json diverges from today and every programmatic caller's parse shifts.

**Scenarios (TS-05 / TS-08 / ADR-003 table):**
1. Clean passthrough `format=json` → **no** `resolution` key (byte-identical to pre-vnc-042).
2. `Followed` → `{"status":"followed","requested_id":X,"returned_id":Y}` present.
3. `DeadEnd` → `{"status":"no_active_successor","requested_id":X}`.
4. `AsStoredDeprecated` → `{"status":"as_stored_deprecated","requested_id":X,"superseded_by":Z}`.

**Coverage Requirement:** presence/absence assertion on the `resolution` key for all four ADR-003 cases; clean case ties back to TS-01 byte-identity.

### R-08: Escape-hatch footer on a NULL-`superseded_by` deprecated entry
**Severity:** Med · **Likelihood:** Med · **Impact:** FR-07/AC-03 append `deprecated; superseded by #{X} …`. But an **orphaned deprecated** or **quarantined** entry has `superseded_by IS NULL` — there is no `#{X}`. Requesting it with `follow_supersessions=false` must not render a malformed footer (`superseded by #` / `#null` / panic on `unwrap`). This is the intersection of the escape hatch (ADR-001) and the dead-end shape (ADR-002) that neither ADR fully pins.

**Scenarios (TS-06 extension):**
1. `context_get(orphaned_deprecated, follow_supersessions=false)` → as-stored content + a **well-formed** footer variant when `superseded_by` is NULL (e.g. footer omitted or a "deprecated; no successor recorded" form). Assert no `unwrap`/panic, no `#null`.
2. `context_get(active_entry, follow_supersessions=false)` → verbatim, **no** footer (footer only for deprecated).
3. `context_get(B_deprecated_with_successor, follow_supersessions=false)` → footer names the real `#{superseded_by}` (baseline TS-06).

**Coverage Requirement:** the NULL-`superseded_by` as-stored case is exercised and produces a well-formed (or absent) footer, no panic (C-4).

### R-09: Default-contract flip on non-code durable-id callers (accepted)
**Severity:** High · **Likelihood:** Med · **Impact:** SR-01 — memory files, agent/skill defs, edges, prior-session ids that intentionally passed a deprecated id now silently get the terminal. Non-code consumers are outside any test harness (untestable). **Accepted product bet, LOCKED #843.**

**Mitigation (testable proxy):** FR-13 tool-description update (`tools.rs:947-948`) documents the new default and the discoverable escape hatch (C-5, NFR-08, #4303).

**Coverage Requirement:** assert the tool-description string mentions `follow_supersessions` and the default; **flag for human** that behavioral coverage for non-code consumers is impossible by design.

### R-10 / R-12: Accepted footguns (documented, not gated)
- **R-10 (SR-06):** graph `resolve_supersessions=false` vs get `follow_supersessions=true` — distinct verb mitigates; ADR-001 documents the divergence. No test; review-time awareness.
- **R-12 (SR-07 / NG-1):** resolved terminal's edge *targets* stay unresolved (old id+title). The resolution notice makes it legible. Deferred follow-up.

---

## Integration Risks

- **`id → effective_id` threading (R-03):** the resolved id must flow to **both** `entry_store.get(effective_id)` **and** `build_edges_view(store, effective_id)`. A partial swap (fetch terminal, edges on requested) is the highest-probability integration defect. Single-fetch (no double-read) per ARCHITECTURE data flow.
- **Type flow (clean):** `validated_id(params.id) -> u64` feeds `follow_to_current(&store, id) -> Option<u64>` with no cast (`tools.rs:977` → helper). Confirm no `as`/`try_into` introduced.
- **Terminal-fetch race:** `follow_to_current` returns `Some(terminal)`, then `entry_store.get(terminal)` — if the terminal is deleted/corrected between the two reads, the fetch fails → FAIL-LOUD `ServerError::Core` (not a dead-end flag). Scenario worth a note; low likelihood.
- **Reused-primitive invariants:** the 50-hop cap and `status=0` active-terminal guard inside `follow_to_current`/`query_current_terminal` are load-bearing and untouched (C-3, #4538). `graph_queries_tests.rs` stays the authority for chain correctness; vnc-042 adds only the `context_get`-level exercises.
- **Store-layer false positives (do NOT migrate):** read-back-after-deprecate tests call `store.get()` (stays as-stored) — they do **not** break and are **out of scope** (#5383). Counting them over-scopes the acceptance map.

## Edge Cases

- Requested id **is** the active terminal → clean passthrough, no notice (TS-05).
- Requested id is a non-deprecated, non-superseded active entry → clean passthrough.
- Requested id itself **quarantined** (not deprecated) with no successor → `None` → dead-end flag on requested id.
- **Exactly 50 vs 51 hops** — boundary of the cap; 50 resolves, 51 → dead-end.
- **Cycle / self-`superseded_by`** — cap trips, no infinite loop (R-04.4).
- Orphaned deprecated + `follow_supersessions=false` + NULL `superseded_by` (R-08).
- Non-existent id → primary fetch error, FAIL-LOUD (unchanged behavior).

## Security Risks

`context_get` accepts two untrusted inputs from the MCP caller: `id` and `follow_supersessions`.
- **`id`** — integer (`deserialize_i64_or_string`, unchanged). Feeds the recursive-CTE walk; **DoS bound = the 50-hop cap** (C-3) — a malicious deep/cyclic chain cannot cause unbounded traversal. SQL is parameterized (no injection).
- **`follow_supersessions`** — plain `Option<bool>`; **must not** become a `deserialize_*_or_string` field (reintroduces the #3728 quoted-scalar coercion class). No new attack surface.
- **Blast radius / info-exposure:** the escape hatch (`=false`) returns deprecated/quarantined content **by exact id** — but `context_get` already exposes by-id reads; no new data is reachable that a direct id read did not already reach. Resolution only changes *which* already-authorized entry is returned. No path traversal, no deserialization of untrusted structured payloads. Net: **low**, provided the bool stays plain and the hop cap is untouched.

## Failure Modes

| Condition | Required behavior | Verifies |
|-----------|-------------------|----------|
| Primary fetch fails (`entry_store.get(effective_id)`) | mapped `ServerError::Core`, returned — never empty | FR-14, C-4 |
| `build_edges_view` fails | FAIL-LOUD, same mapping (existing FR-19, `tools.rs:984-987`); resolution does not soften | FR-14 |
| `follow_to_current` internal store error | collapses to `None` → dead-end **flag** (loud), never silent success | R-04, ADR-002 |
| Chain dead-ends (orphaned/quarantined/>50) | non-empty result + loud non-active flag, returned id = requested | AC-04, FR-08 |
| Any non-test path | **no `.unwrap()`**; errors via project error type + `.map_err` | C-4 |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution / Coverage |
|-----------|------------------|----------------------|
| **SR-01** default-behavior contract flip | R-09 | Accepted product bet (LOCKED #843). Testable proxy: FR-13 tool-description (C-5, NFR-08). Non-code consumers untestable — flagged for human. |
| **SR-02** test/CI blast radius (top-tier) | R-01, R-03, R-06, R-07, R-11 | Authoritative coverage plan = this doc. Canaries TS-01/TS-02 stay green (regression guard); TS-03 classification tracked pre-delivery; store-layer read-backs excluded as false positives (#5383); fixtures verified none encode get responses. |
| **SR-03** include_edges resolved-id mismatch | R-03 | ADR-003 keys edges on `effective_id`. TS-03 review pass + hopped-get edge assertion (§R-03). |
| **SR-04** notice injection point | R-01, R-07 | ADR-003 injects only in `format_single_entry_with_note`; byte-identity canary TS-01 + `resolution`-key presence/absence guard the invariant. |
| **SR-05** dead-end / fail-loud correctness | R-04 | ADR-002 option (a): return requested id + loud flag. TS-07 covers orphaned/quarantined/>50-hop/cycle/store-error; cap preserved (C-3, #4538). |
| **SR-06** graph-vs-get naming/default divergence | R-10 | ADR-001 accepts divergence (distinct verb `follow_*`, shared noun). Documented, not gated — review-time awareness. |
| **SR-07** mixed-resolution (NG-1 sharp edge) | R-12 | Accepted; resolution notice makes the asymmetry legible. Neighbor-target resolution deferred. |

## CI Notes (blast-radius, Linux-only local gates)

- Local gates 3a/3b/3c run **Linux-only**; the **JS CI matrix (incl. Windows) is the cross-platform gate**. Rust validation = protocol cargo-test gates + release workflows **by design** — a missing Rust CI job is **not** a gap.
- **CI-only exposure (R-11):** JS/E2E assert **transport framing only**, not get response content (verified SR-02). If the JS edge client mirrors a `GetParams` schema, the additive `follow_supersessions` field may need parity there — a mismatch surfaces only in the JS CI matrix, not local Linux gates. **Budget one post-PR CI round-trip.**
- **Fixture guard (#5099 / vnc-038 mode):** no fixtures/goldens are known to encode `context_get` response content. IF one encoding the OLD default is found at delivery, migrating it is **development work to be FLAGGED, not silently narrowed** by a file-scoped agent. This is the surprise this document exists to prevent.

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | Byte-identity canary green (TS-01/02) + strip-compare; **behavioral** default-on test (field absent → resolves), field-present true/false, no coercion |
| High | 4 (R-03, R-04, R-07, R-09) | Hopped-get terminal-edge assertion + TS-03 classification; orphaned/quarantined/>50-hop/cycle/store-error dead-end flags; `resolution`-key presence/absence 4 cases; tool-description string |
| Medium | 4 (R-05, R-06, R-08, R-10) | Warnings-clean build + canonical call-site; AC-01 body-equivalence across formats; NULL-`superseded_by` footer well-formed/no-panic; divergence documented |
| Low | 2 (R-11, R-12) | JS CI matrix round-trip budgeted; NG-1 asymmetry legible via notice |

**Authoritative regression guards (must stay green, edits = FLAG events):** TS-01 byte-identity canary · TS-02 ~15 shape tests · NFR-06 param-additivity · `graph_queries_tests.rs` hop-cap/orphan-guard suite.

## Knowledge Stewardship
- Queried: `context_search` for risk patterns / gate-rejection lessons — surfaced #5383 (blast-radius partitioning; store-layer read-backs are false positives — applied to SR-02 traceability + CI notes), #3774/#3817 (serde-default dual-site silent failures — elevated R-02 to Critical), #4876 (verify error propagation empirically — R-04 store-error-collapse scenario), #5099 (FLAG don't narrow — fixture guard), #4303 (tool descriptions must not lie — R-09 proxy).
- Stored: nothing novel to store — the governing pattern (#5383) already exists and is vnc-042-specific; the serde-default-footgun pattern (#3774/#3817) is already generalized. No cross-feature (2+) risk pattern newly visible.
