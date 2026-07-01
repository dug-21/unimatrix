# SPECIFICATION — vnc-042

**Feature:** `context_get` resolves superseded (deprecated) entries to their current version by default.
**Source:** `product/features/vnc-042/SCOPE.md` · **Risk:** `product/features/vnc-042/SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-07)
**Tracking:** GH #843 (product behavior + AC-1..AC-7 LOCKED).

> Two decisions are owned by the architect's ADR (running in parallel) and are referenced here as **"per ADR"**, not decided in this spec:
> - **ADR-D1 (SR-03):** which entry's edge *list* a resolved get returns (requested id vs. resolved terminal id).
> - **ADR-D2 (OQ-3):** the `format="json"` notice/flag rendering shape (structured field vs. prepended string).
> Acceptance criteria AC-07 (edges) and AC-02/03/04 (json rendering) are written to bind to whatever the ADR rules.

---

## 1. Objective

`context_get(id)` currently performs a raw by-ID read (`entry_store.get(id)`) with no status check, silently returning stale content when `id` points at a deprecated entry. This feature adds one parameter, `follow_supersessions: bool` (default `true`), so that by default a deprecated id resolves to its active terminal — full content, same shape — while `false` returns the entry exactly as stored for lookback/audit. The change is a surgical single-tool contract change in `tools.rs` reusing existing supersession-resolution machinery (`follow_to_current`); no schema, SQL, or other-tool changes.

---

## 2. Domain Model / Ubiquitous Language

| Term | Definition |
|------|------------|
| **Supersession** | The `context_correct`-written relationship where a deprecated entry A points forward via `superseded_by` to its replacement B. A chain A→B→C may span multiple hops. |
| **Active terminal** | The entry at the end of a supersession chain with `status = Active (0)` and `superseded_by IS NULL`. The canonical "current version" of a corrected entry. |
| **Follow-to-current** | Walking the supersession chain from a requested id to its active terminal via `follow_to_current` (50-hop cap). |
| **Clean passthrough** | The requested id is already the active terminal (or is a non-deprecated entry with no successor): the entry is returned with **no** resolution notice. |
| **Resolution notice** | The one-line `↻ Requested #{id} (deprecated) → returning current version #{terminal}.` emitted **only when a hop occurred**. |
| **Exact / as-stored** | The `follow_supersessions=false` escape-hatch result: the requested entry returned verbatim regardless of status, for provenance/audit/lookback. |
| **Deprecated footer** | The non-intrusive `deprecated; superseded by #{X} (pass follow_supersessions=true to follow)` appended to an as-stored result when the requested entry is deprecated. |
| **Orphaned dead-end** | A chain that terminates on a non-active entry with `superseded_by IS NULL` and `status != Active` — no active successor exists. |
| **Quarantined dead-end** | A chain terminating on a quarantined entry. Treated identically to orphaned for the AC-04 fail-loud path. |
| **Non-active flag** | The loud marker attached when resolution dead-ends (orphaned/quarantined/>50 hops): a result is still returned, never empty, never silent. |
| **Byte-identity invariant** | ADR-003 guarantee that `format=null` json output is byte-for-byte identical to the base object (canary test `test_none_json_byte_identical_to_base_object`). Notices must attach in the handler wrapper, never inside `format_single_entry`. |

---

## 3. Functional Requirements

Each is testable. Verification named in §5 / §6.

- **FR-01** `GetParams` gains a field `follow_supersessions: Option<bool>` with `#[serde(default)]`. Semantics: `None` (field omitted) ⇒ **follow** (default-on); `Some(true)` ⇒ follow; `Some(false)` ⇒ as-stored. The **handler owns the default** — it must NOT rely on a bare `#[serde(default)] bool`, which would resolve to Rust `Default = false` and silently invert AC-06 to default-OFF. Per ADR-001. *(AC-06, C-2)*
- **FR-02** When `follow_supersessions=true` and the requested `id` is deprecated with a reachable active terminal, the handler fetches and returns the **active terminal's full content**, in the same response shape as a direct get of that terminal. *(AC-01)*
- **FR-03** Resolution is performed by calling the existing `follow_to_current(&store, id)` primitive; no new chain-walk is implemented. *(AC-05, C-1)*
- **FR-04** When resolution hops (`follow_to_current` returns `Some(terminal)` with `terminal != id`), the response carries the one-line resolution notice `↻ Requested #{id} (deprecated) → returning current version #{terminal}.`. *(AC-02)*
- **FR-05** When no hop occurs (`Some(terminal)` with `terminal == id`, i.e. requested id is already the active terminal or a non-deprecated entry), the response carries **no** resolution notice (clean passthrough). *(AC-02)*
- **FR-06** When `follow_supersessions=false`, the handler fetches the requested entry **exactly as stored** for any status (no follow, no status filter). *(AC-03)*
- **FR-07** When `follow_supersessions=false` and the requested entry is deprecated, the response appends the non-intrusive deprecated footer pointing at its `superseded_by` target and naming the opt-in value. When the deprecated entry is **orphaned/quarantined** (`superseded_by IS NULL`), there is no successor id to render — the footer MUST be well-formed or absent with **no panic and no malformed `#{}`**; exact wording per **ADR-003**. *(AC-03, AC-08)*
- **FR-08** When `follow_supersessions=true` and `follow_to_current` returns `None` (orphaned dead-end / quarantined dead-end / chain > 50 hops), the handler returns a result carrying a **loud non-active flag** — never empty, never silent. The entry returned on the `None` path is the **originally-requested id** (OQ-2 recommendation (a): `follow_to_current` discards the stop-id, so the cheap path re-fetches the requested id and flags it; no new walk). *(AC-04)*
- **FR-09** The resolution notice, deprecated footer, and non-active flag are attached in the **handler wrapper around `format_single_entry`**, mirroring `format_store_success_with_note` — **never** inside `format_single_entry`. *(SR-02a, SR-04, byte-identity invariant)*
- **FR-10** `follow_supersessions` composes orthogonally with `format` (null/markdown/json) and `include_edges`: resolution selects *which* entry; `format` renders it; `include_edges` surfaces edges. *(AC-07)*
- **FR-11** For a resolved get, the entry whose edge list is returned under `include_edges` is determined **per ADR-D1 (SR-03)** — either recomputed on the resolved terminal id or documented as keyed on the requested id. The handler wires edge assembly to the id the ADR designates. *(AC-07, SR-03; ADR-owned)*
- **FR-12** For `format="json"`, the resolution notice / deprecated footer / non-active flag rendering shape (structured field vs. prepended string) follows **ADR-D2 (OQ-3)**, giving programmatic callers a stable contract. *(AC-02/03/04, OQ-3; ADR-owned)*
- **FR-13** The `context_get` tool-description strings (`tools.rs:947-948`) are updated to document the new default and the `follow_supersessions` parameter (including the escape-hatch value). *(C-5, SR-01, #4303)*
- **FR-14** Post-primary-read failures (edge assembly, terminal fetch) remain FAIL-LOUD via the project error type + `.map_err`, consistent with existing `include_edges` handling (`tools.rs:984-987`). No `.unwrap()` in non-test code. *(C-4)*

---

## 4. Non-Functional Requirements

- **NFR-01 (Backward-compat, handler-owned default)** A pre-vnc-042 tool call omitting `follow_supersessions` MUST deserialize to `None` and take the **follow** path via the handler-owned default (NOT Rust `bool::default()==false`). Verified **behaviorally**: field ABSENT from the JSON → `context_get(deprecated_id)` resolves to the terminal (AC-06), not by a serde field-value round-trip alone. *(AC-06, C-2, ADR-001)*
- **NFR-02 (No type-coercion fragility)** `follow_supersessions` is `Option<bool>` with `#[serde(default)]` — it MUST NOT reintroduce the quoted-scalar coercion class of bug (#3728). Not a `deserialize_*_or_string` field. Verified by deserialization tests with the field present (`true`/`false`) and absent (`None`).
- **NFR-03 (No new chain-walk / reuse-only)** No new supersession traversal code is added; resolution routes only through `follow_to_current` (+ fallback fetch). Verified by code review and absence of any new recursive/loop walk. *(AC-05, C-1, #4468)*
- **NFR-04 (Hop cap + orphan guard preserved)** The 50-hop cap and the `status=0` active-terminal guard in the underlying primitives are unchanged and remain load-bearing. Verified by the existing `graph_queries_tests.rs` suite staying green + a >50-hop / orphaned dead-end test exercising `context_get`. *(C-3, #4538)*
- **NFR-05 (Byte-identity preserved)** `test_none_json_byte_identical_to_base_object` MUST still pass unchanged after the feature. This is the canary confirming notices attach in the wrapper, not the formatter. *(SR-02a, SR-04)*
- **NFR-06 (Param additivity)** No existing `GetParams` field is removed or retyped; `follow_supersessions` is purely additive. Verified by `test_get_params_no_existing_field_removed_or_retyped`. *(SR-02c)*
- **NFR-07 (Single-tool blast radius)** No schema, SQL, or changes to `context_search`, `context_lookup`, or `context_graph` (incl. its `resolve_supersessions` param/default). Verified by diff scope review against NG-3/NG-4/NG-5.
- **NFR-08 (Escape-hatch discoverability)** The as-stored opt-out MUST be discoverable in the tool description so audit/lookback/durable-id callers can find it. *(SR-01, C-5)*

---

## 5. Acceptance Criteria

AC-01..AC-07 carried verbatim from GH #843 (LOCKED), with the old `version` enum replaced by `follow_supersessions`. AC-08 is spec-derived (R-08) and does not alter the locked set. Each has a concrete verification method.

| AC | Criterion | Verification Method |
|----|-----------|---------------------|
| **AC-01** | `context_get(id)` where `id` is deprecated returns the **full content of the active terminal**, identical in shape to a direct get of that terminal. | Integration/handler test: correct A→B (B active terminal). `context_get(A)` (default) and `context_get(B)` produce structurally identical entry payloads (same fields, terminal's content). Assert returned entry id == B and body == B's stored content. |
| **AC-02** | When resolution hops, the response carries the one-line `↻ Requested #X (deprecated) → returning current version #Y` notice; when no hop occurs, there is no notice (clean passthrough). | Two-case test: (hop) `context_get(A)` response contains the exact notice string with X=A, Y=B; (no-hop) `context_get(B)` (active terminal) contains **no** notice substring. For `format="json"`, assert notice presence/absence per ADR-D2 shape. |
| **AC-03** | `follow_supersessions=false` returns the entry **exactly as stored** for any status, with the deprecated-footer pointer when the requested entry is deprecated. | Test: `context_get(A, follow_supersessions=false)` returns A's stored content (id==A, deprecated status) plus footer `deprecated; superseded by #B (pass follow_supersessions=true to follow)`. `context_get(B, follow_supersessions=false)` returns B verbatim with no footer. |
| **AC-04** | A chain terminating on a non-active entry (orphaned / quarantined) returns a result with a **loud non-active flag** — never empty, never silent. Per **ADR-002** the entry returned is the **originally-requested** id (the stop-id is discarded by `follow_to_current`). Mirrors `current`-mode R-20 guard. | Test: construct a deprecated entry whose chain dead-ends on an orphaned/quarantined non-active entry (and separately a >50-hop chain). `context_get(id)` (default) returns a non-empty result whose entry id == the originally-requested id, carrying the loud non-active flag; assert non-empty, returned-id == requested id, and flag present. |
| **AC-05** | Resolution reuses `follow_to_current` (hop cap + fallback); **no new chain-walk** implementation is added. | Code review + call-site assertion: handler calls `follow_to_current`; no new recursive CTE or in-memory walk introduced. Existing `follow_to_current` / `query_current_terminal` tests remain the authority for chain correctness. |
| **AC-06** | Resolution is **on by default** when the parameter is omitted (the contract change is the new default). | **Behavioral** test (authoritative): a tool call with `follow_supersessions` ABSENT from the JSON → `context_get(A)` (A deprecated) resolves to terminal B (== AC-01 default path). This guards the ADR-001 handler-owned-default requirement (a bare `#[serde(default)] bool` would default-OFF and silently pass a serde round-trip while failing here). |
| **AC-07** | `follow_supersessions` composes correctly with `format` and `include_edges` (orthogonal). | Matrix test: for deprecated A→B, run `context_get(A)` across `format ∈ {null, markdown, json}` and `include_edges ∈ {omitted, true, false}`; assert resolution to B holds in every cell, `format` renders B, and `include_edges` behaves per ADR-D1 (edges keyed on the ADR-designated id). Assert byte-identity canary unaffected for `format=null`. |
| **AC-08** *(spec-derived, R-08)* | `follow_supersessions=false` on an **orphaned/quarantined deprecated** entry (`superseded_by IS NULL`) returns the entry as-stored with the deprecated footer **well-formed or absent** — no panic, no malformed `#{}`. Footer wording per **ADR-003**. | Test: construct a deprecated entry with `superseded_by = NULL`. `context_get(id, follow_supersessions=false)` returns the entry verbatim; assert no panic, no `#{}` / empty-id substring in the footer, and footer matches the ADR-003-pinned form (or is absent). |

---

## 6. Test Surface as Tracked Requirements (SR-02 — human-flagged priority)

The value of this design work is capturing the test surface as **explicit, tracked requirements** so nothing surfaces at delivery time (cf. vnc-038 CI-red on a stale fixture). Per Unimatrix #5383, the surface is partitioned by layer; store-layer read-back-after-deprecate tests call `store.get()` (stays as-stored) and are **false positives — they do NOT break** and are not in scope for migration. The genuine live clusters and required new tests:

### 6.1 Must-still-hold canaries (regression guard)
- **TS-01 (byte-identity canary)** `test_none_json_byte_identical_to_base_object` (`response/mod.rs:~368`, ADR-003, referenced `tools.rs:999`) MUST stay green. Breaks only if a notice is injected inside `format_single_entry`. FR-09 places the notice in the handler wrapper to protect this. *(NFR-05, SR-02a)*
- **TS-02 (format_single_entry shape tests, ~15 sites)** `response/mod.rs:296-469`. Exact-shape assertions for the formatter output. These MUST stay green — the formatter is unchanged; the notice attaches outside it. Any breakage here is a signal the notice was mis-placed (FR-09 violated). *(SR-02b)*
- **TS-03 (include_edges contract + param-additivity, ~18 tests)** `get_edges_tests.rs` (`tools.rs:5630-5690`), incl. `test_get_params_no_existing_field_removed_or_retyped` (NFR-06). These assert edges-of-queried-id. Behavior post-change depends on ADR-D1 (FR-11): if the ADR keys edges on the resolved terminal, this cluster requires **review and possible migration** — this is development work, tracked here, **not a delivery-time surprise**. If the ADR keeps edges on the requested id, the cluster stays green. *(SR-02c, SR-03)*

### 6.2 New tests required by the change
- **TS-04 (default resolves deprecated → terminal)** — AC-01 / AC-06 default path.
- **TS-05 (clean passthrough, no notice)** — AC-02 no-hop case.
- **TS-06 (`follow_supersessions=false` exact-as-stored + footer)** — AC-03; includes the **orphaned/quarantined `superseded_by IS NULL`** footer edge case (AC-08 / R-08): no panic, no malformed `#{}`, footer per ADR-003.
- **TS-07 (dead-end fail-loud flag)** — AC-04, orphaned/quarantined and >50-hop; asserts returned-id == originally-requested id (ADR-002).
- **TS-08 (orthogonality with `format` and `include_edges`)** — AC-07 matrix.
- **TS-09 (backward-compat, handler-owned default)** — AC-06 / NFR-01 / NFR-02. **Behavioral**: field ABSENT ⇒ `context_get(deprecated_id)` resolves to terminal (guards against `Option<bool>` mis-implemented as default-OFF `bool`); plus field-present (`true`/`false`) deserialization.

**Explicit fixture note:** no fixtures/goldens are known to encode `context_get` response content (verified in SR-02), so fixture migration is expected minimal. IF a fixture encoding the OLD default (as-stored for deprecated ids) is discovered at delivery, migrating it is **development work to be done, not a defect** — file-scoped delivery agents MUST FLAG such an adjacent break rather than silently narrow a test (#5099). This is the vnc-038 failure mode and is the reason the surface is enumerated now.

---

## 7. User / Agent Workflows

1. **Default read (durable id, possibly stale)** — Agent calls `context_get(id)` from a memory file / edge / prior session. If `id` is deprecated, it transparently receives the current terminal plus a resolution notice explaining the hop. No behavior change needed by the caller.
2. **Audit / provenance / lookback** — Caller passes `follow_supersessions=false` to inspect the entry exactly as stored, including deprecated content, with a footer pointing at the successor.
3. **Dead-end diagnosis** — Caller requests a deprecated id whose chain has no active terminal; receives the found entry with a loud non-active flag rather than silent stale content or an empty result.
4. **Composed read** — Caller combines `follow_supersessions` with `format` and `include_edges`; resolution, rendering, and edge surfacing behave independently.

---

## 8. Constraints

- **C-1** Reuse `follow_to_current` / `query_current_terminal`; no reimplemented chain-walking. *(AC-05, #4468)*
- **C-2** `#[serde(default)]` on `follow_supersessions` → omitted field deserializes to default-on. *(AC-06)*
- **C-3** 50-hop cap and `status=0` orphaned-terminal guard are load-bearing; MUST NOT be weakened. *(SR-05, #4538)*
- **C-4** No `.unwrap()` in non-test code; errors via project error type + `.map_err`. Post-primary-read failures FAIL-LOUD. *(SR-05)*
- **C-5** Update `context_get` tool-description strings to document the new default and parameter (a description that lies to agents is a known hazard, #4303). *(SR-01)*
- **C-6** Requires the ADR (architect authority). ADR MUST rule on ADR-D1 (SR-03, edge-list id), ADR-D2 (OQ-3, json notice shape), and the graph-vs-get naming/default divergence (SR-06). *(This spec binds to those rulings.)*
- **C-7** Notice injection point is the handler wrapper, not `format_single_entry` (protects byte-identity). *(SR-04)*

---

## 9. Dependencies

- **Existing primitives (present, no upstream blocker):** `follow_to_current` (`graph_read_neighbors.rs:36-55`), `query_current_terminal` (`graph_queries.rs:161-201`), `supersedes`/`superseded_by` columns (`schema.rs:67,69`; `db.rs:554-555`) written by `context_correct`.
- **Surface to change:** `context_get` handler (`tools.rs:950-1052`), `GetParams` struct (`tools.rs:246-274`), tool-description strings (`tools.rs:947-948`), notice-attachment pattern precedent `format_store_success_with_note` (`tools.rs:936`).
- **ADR (C-6):** required before delivery; owns ADR-D1, ADR-D2, and the SR-06 naming/default divergence ruling.
- **Enables (out of scope here):** the deferred follow-up resolving stale neighbor targets inside `include_edges` (NG-1) can build on the resolution wiring.

---

## 10. NOT in Scope

- **NG-1** Resolving stale **neighbor/edge targets** inside `include_edges` — deprecated targets still show old id+title unresolved. Only the **requested** entry resolves. (SR-07 sharp edge accepted; ADR-D1 pins only *which entry's edge list* is returned, not neighbor-target resolution.)
- **NG-2** Multi-entry / chain / evolution view on `context_get` — chain lookback stays in `context_graph` mode `chain`.
- **NG-3** No change to `context_search` or `context_lookup`.
- **NG-4** No change to `context_graph` or its `resolve_supersessions` parameter, default, or semantics.
- **NG-5** No schema / storage change.
- Migration of **store-layer read-back-after-deprecate tests** — false positives; they exercise `store.get()` which stays as-stored and do not break (#5383).

---

## 11. Open Questions (for architect / user)

- **OQ-A (→ ADR-D1, SR-03):** Which entry's edge *list* does a resolved get return — recomputed on the resolved terminal id, or kept keyed on the requested id with the mismatch documented? Determines whether the ~18 `get_edges_tests.rs` assertions need migration (TS-03). *Architect to rule.*
- **OQ-B (→ ADR-D2, OQ-3):** For `format="json"`, is the resolution notice / footer / non-active flag a structured field (e.g. `resolution_notice`) or a prepended string? Pins the programmatic contract. *Architect to rule.*
- **OQ-C (→ ADR, SR-06):** Accept the `follow_supersessions` (default true) vs. graph's `resolve_supersessions` (default false) naming/default divergence with documented rationale, or standardize later? *Architect to rule.*
- **OQ-D (RESOLVED — ADR-002):** the dead-end (`None`) path returns the **originally-requested** id with the loud flag (not the discarded stop-id), preserving AC-05 "no new chain-walk." AC-04 wording aligned accordingly. No longer open.

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #5383 (vnc-042 blast-radius partitioning: store-layer read-back tests are false positives; real surface = byte-identity canary + edge-projection + param-additivity), #4460 (vnc-017 terminal-active resolution semantics), #3728 (context_get quoted-id serialization hazard → keep plain bool), #4468/#4538 (recursive-CTE + status=0 guard load-bearing), #4303 (tool descriptions must not lie). Applied to §6 test surface, NFR-02/03/04, C-3/C-5.
- Read-only tier — no storage; spec decisions are feature-specific.
