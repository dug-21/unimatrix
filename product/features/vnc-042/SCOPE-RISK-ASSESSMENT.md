# Scope Risk Assessment: vnc-042

Default-behavior change to the most-used read tool (`context_get`). Code delta is small; the risk is contract + test-surface, not implementation size. Blast radius enumerated at design time per vnc-038 lesson (Unimatrix #5099).

## Technology / Contract Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | **Default-behavior contract flip** on the most-used read tool. Durable-id callers that intentionally pass a *deprecated* id expecting as-stored content — memory files, agent/skill defs, stored docs, another entry's edge, prior-session ids — now silently receive the resolved terminal. Non-code consumers are out of any test harness. | High | Med | ADR (C-6) must state the new default explicitly; spec must make the escape hatch (`follow_supersessions=false`) discoverable in the tool description (C-5) so audit/lookback callers can opt out. |
| SR-02 | **Test/CI blast radius (TOP-TIER, per human directive).** Enumerated against the tree — *narrower than feared, but with specific live surfaces*: (a) **byte-identity invariant** `test_none_json_byte_identical_to_base_object` (`response/mod.rs:~368`, ADR-003, referenced at `tools.rs:999`) breaks if a notice is injected inside `format_single_entry`; (b) `format_single_entry` exact-shape tests (~15 sites, `response/mod.rs:296-469`); (c) `include_edges` contract + additivity tests (`tools.rs:5630-5690`, incl. `test_get_params_no_existing_field_removed_or_retyped` NFR-4). **Verified LOW:** store-layer read-back-after-deprecate tests call `store.get()` (stays as-stored) — they do **not** break; the handler has **zero** direct Rust call sites; **no fixtures/goldens encode get responses**; JS/E2E assert transport framing only, not content. | High | Med | Enumerate the ~15 shape + ~18 edge tests + byte-identity canary as **acceptance-map/task-decomposition rows with their own coverage**, NOT delivery-time surprises (#5099). Fixture migration here is minimal, but the *classification work* is real and must be tracked. |
| SR-03 | **`include_edges` resolved-id mismatch.** Handler builds edges from the *original* `id` (`build_edges_view(store, id)`, `tools.rs:991`). If resolution swaps the returned entry to the terminal but edges stay keyed on the original id, the response shows terminal content with the wrong entry's edges. NG-1 defers neighbor resolution but does **not** say which entry's edge *list* the resolved get returns. | High | Med | Spec must pin: resolved get returns the **terminal's** edges (recompute on terminal id), or explicitly document the mismatch. ~18 tests in `get_edges_tests.rs` assert edges-of-queried-id and will need review. |
| SR-04 | **Notice injection point** (`↻` notice / deprecated footer, AC-2/3/4). If prepended inside `format_single_entry`, it breaks byte-identity + json shape (SR-02a). | Med | Med | Architect: inject notice in the handler wrapper around format, mirroring `format_store_success_with_note` (`tools.rs:936`); resolve OQ-3 (json → structured field vs prepended string) so programmatic callers keep a stable contract. |
| SR-05 | **`follow_to_current` dead-end / fail-loud correctness.** Primitive returns `None` on orphaned-deprecated / quarantined terminal / >50 hops, discarding the stop-id. AC-4 requires a loud non-active flag, never empty — a silent-empty return would violate vision principle #5 (graceful degradation, not broken behavior). OQ-2 (which entry is returned on `None`) is unresolved. | Med | Med | Spec/architect confirm OQ-2 (recommend: return originally-requested id + loud flag). Preserve 50-hop cap and `status=0` guard (C-3, #4538). |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | **Graph-vs-get naming/default divergence.** `context_graph` exposes `resolve_supersessions` default **false**; vnc-042 adds `follow_supersessions` default **true** — same concept, opposite default, near-identical name. Reviewer confusion / footgun. | Med | High | ADR (C-6) must explicitly rule: accept divergence with rationale, or standardize. Distinct verb (`follow_*`) already mitigates the same-name/opposite-default trap; document the choice. |
| SR-07 | **Mixed-resolution response (NG-1 sharp edge).** Requested entry resolves to terminal, but its `include_edges` targets still show deprecated old id+title. User-facing inconsistency, accepted as scoped follow-up. | Low | High | Confirm the resolution notice makes the asymmetry legible; leave neighbor resolution to the deferred follow-up. |

## Assumptions

- **AC-06 tolerance** (SCOPE Goals 1): all durable-id consumers can absorb silently-resolved content. Untestable for non-code consumers (memory/agent files) — accepted as a product bet, mitigated by the escape hatch.
- **Reuse soundness** (SCOPE §Background, AC-05): `follow_to_current` + `query_current_terminal` are the sole correct primitives and need no new chain-walk. Verified present; store-layer tests exist (`graph_queries_tests.rs:316-360`).
- **Fixture-migration is minimal** (verified): no fixtures/goldens encode get responses. If this assumption is wrong at delivery, it becomes the #5099 failure mode — CI red on stale fixture after all local (Linux-only) gates pass.

## Design Recommendations

1. **SR-02 → acceptance map:** enumerate the byte-identity canary + ~15 shape tests + ~18 edge tests as explicit coverage rows now; instruct file-scoped delivery agents to FLAG (not silently narrow) any adjacent test breakage (#5099).
2. **SR-04 → architecture:** place the notice in the handler wrapper, not in `format_single_entry`, to preserve the ADR-003 byte-identity invariant.
3. **SR-03 → spec:** pin which entry's edges a resolved get returns.
4. **SR-05/SR-06 → ADR (C-6):** rule on OQ-2 (dead-end entry) and the graph-vs-get default divergence.
5. **CI budget:** local gates are Linux-only; JS CI matrix (incl. Windows) is the cross-platform gate. JS tests don't assert get content here, so CI-only exposure is low — but budget one post-PR CI round-trip. (Rust validation = protocol cargo-test gates + release workflows by design — not a CI gap.)
