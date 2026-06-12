# Test Plan — `context_correct` handler (step 8b′ + `edges_carried` ack)

> Component: the `context_correct` handler in `crates/unimatrix-server/src/mcp/tools.rs`
> (~:1015) — step 8b′ insertion between 8b (`params.edges` write) and 8c (incoming redirect),
> and `edges_carried` threaded into the ack (~:1162). End-to-end integration tests live inline
> in `mod tests`; extend vnc-015/vnc-017 correction fixtures (NFR-06).
>
> Risks owned: **R-04** (pipeline order — High), **R-07** (tick staleness — Medium),
> **R-10** (shed targets Active new id — Low), plus the integration halves of **R-02** (ack
> envelope) and **R-03** (AC-04 mix). Pipeline order (load-bearing — ADR-001):
> `8 → 8b → 8b′ → 8c → 9 → 10`.

---

## AC-01 / AC-02 — carry by default, attach to new id

### `test_correct_carries_eligible_outgoing_by_default` (AC-01) — REQUIRED
- **Arrange**: seed A with eligible outgoing edges (`Supports → X`, `Advances → Y`).
- **Act**: `context_correct(A→B)` with `edges` **omitted**.
- **Assert**: `graph_edges` has rows with `source_id = B.id` for each original relation/target
  (`(B, X, Supports)`, `(B, Y, Advances)`). No `edges` param was required.

### `test_correct_carried_edges_attach_to_new_id_not_original` (AC-02) — REQUIRED
- **Assert (same fixture as AC-01)**: **no** carried row has `source_id = A.id`; A is
  Deprecated post-correct; all carried rows have `source_id = B.id`.

### `test_correct_goal_advances_vision_root_regression` (AC-03, Goal 5) — REQUIRED
- **Arrange**: seed a goal entry whose **only** edge is `Advances → vision_root`.
- **Act**: `context_correct(A→B)` with `edges` omitted.
- **Assert**: `graph_edges` has `(B.id, vision_root, Advances)`. Reproduces and closes the
  confirmed-live regression.

---

## AC-04 (integration half) — derived classes excluded

### `test_correct_excludes_derived_classes_integration` (AC-04, R-03) — REQUIRED
- **Arrange**: seed A with a mix — `Supports` (eligible) + `Supersedes` + `CoAccess` +
  `Informs`.
- **Act**: `context_correct(A→B)`.
- **Assert**: carried edges on B include `Supports`; **exclude** `Supersedes`, `CoAccess`,
  `Informs`. (Unit predicate half is in query_outgoing_edges.md.)

---

## AC-08 — additive-on-triple composition

### `test_correct_composition_idempotent_repass` (AC-08a) — REQUIRED
- **Arrange**: A has eligible outgoing `Supports → X`.
- **Act**: `context_correct(A→B)` passing `edges = [(X, Supports)]` (identical to the carried
  triple).
- **Assert**: **one** row `(B, X, Supports)` — no duplicate; `edges_carried` **not inflated**
  (the re-passed triple is written by 8b, then UNIQUE-conflicts in 8b′ → not counted).

### `test_correct_composition_additive_new_edge` (AC-08b) — REQUIRED
- **Arrange**: A has `Supports → X`; caller passes a genuinely new `edges = [(Z, Supports)]`.
- **Act/Assert**: both `(B, X, Supports)` (carried) and `(B, Z, Supports)` (passed) exist.

### `test_correct_composition_changed_target_two_edges` (AC-08c) — REQUIRED
- **Arrange**: A has `Advances → X`; caller passes `edges = [(Y, Advances)]` — same
  `(source, relation)`, different target.
- **Act/Assert**: **two** edges coexist on B — `(B, X, Advances)` and `(B, Y, Advances)`
  (correct for multi-target relations; not a replacement).

### `test_correct_omission_does_not_shed` (AC-08, removal-only-via-shed) — REQUIRED
- **Arrange**: A has `Supports → X`.
- **Act**: `context_correct(A→B)` passing `edges = []` (or omitting X).
- **Assert**: `(B, X, Supports)` still present — omission from `edges` does **not** remove a
  carried edge; removal is only via the shed path (FR-05/FR-08).

---

## AC-11 (+ R-02 integration) — `edges_carried` ack envelope

### `test_correct_edges_carried_count_n` (AC-11a) — REQUIRED
- **Arrange**: A has N eligible outgoing edges, none re-passed.
- **Act**: `context_correct(A→B)`.
- **Assert**: response envelope has `edges_carried == N` (actual inserts, NFR-03).

### `test_correct_edges_carried_omitted_when_zero` (AC-11b, R-02 #3) — REQUIRED
- **Arrange**: A has no eligible outgoing edges (or only ineligible classes).
- **Act**: `context_correct(A→B)`.
- **Assert**: the `edges_carried` field is **absent** from the response envelope — not `0`,
  not present-and-zero. (Serde `skip_serializing_if` / Option-None semantics.)

### `test_correct_edges_carried_no_content` (AC-11c, R-02 #4) — REQUIRED
- **Assert (on an N>0 response)**: `edges_carried` carries the integer **only** — no target
  ids, no relation types, no edge identities anywhere in the ack. The count is the sole
  awareness channel (no DB provenance marker, OQ-03).

---

## R-04 — pipeline order (load-bearing)

### `test_pipeline_order_8b_before_8b_prime` (R-04 #2, design-mandated check) — REQUIRED
- **Arrange**: caller passes one `edges` triple that is **also** an eligible outgoing edge of
  A (so both 8b and 8b′ would write the same triple).
- **Act**: `context_correct(A→B)`.
- **Assert**: the triple counts in 8b's accounting, **not** `edges_carried` — i.e.
  `edges_carried` does **not** include the re-passed triple (8b writes first → 8b′ hits UNIQUE
  conflict → not counted). This assertion **fails if 8b′ runs before 8b**, pinning the order.

### `test_pipeline_order_carry_before_redirect_contradicts` (R-04 #3) — REQUIRED
- Reuses the `Contradicts(A,X)` convergence fixture (full assertion in
  run_carry_forward_loop.md `test_carry_redirect_contradicts_converge`); at the handler level
  assert the net `Contradicts(B,X)` is consistent end-to-end (both directions exactly once)
  after a real `context_correct(A→B)` — validating 8b′ runs before 8c.

---

## R-07 — tick-window staleness (lesson #4526)

### `test_carried_edge_visible_depth1_immediately` (R-07 #1) — REQUIRED
- **Arrange**: `context_correct(A→B)` carrying `Supports → X`.
- **Act**: immediately (no tick) read B's edges via a **DB-backed depth-1 read**
  (`query_outgoing_edges` / `neighbors` depth=1).
- **Assert**: the carried edge is visible immediately — DB reads do not wait for a tick.

### `test_carried_edge_bfs_path_after_tick` (R-07 #2/#3) — REQUIRED, tick-disciplined
- **Arrange**: `context_correct(A→B)` carrying an edge; then **force a graph tick/drain**.
- **Act**: assert the carried edge via **BFS path-mode/subgraph** retrieval.
- **Assert**: visible **after** the tick. **In-test comment**: pre-tick BFS invisibility is
  expected (#4526), NOT a carry-forward defect. This is the **only** path-mode assertion in
  the suite — do not add another without a preceding tick (the flake-mis-attribution trap).

---

## R-10 — shed targets the new Active id

### `test_shed_carried_edge_against_new_id` (AC-05) — REQUIRED
- **Arrange**: `context_correct(A→B)` carries edge E (`B → target`).
- **Act**: `context_edge remove` with `source_id = B.id` for E.
- **Assert**: E absent from `graph_edges` afterward. Shed works against the new Active entry.

### `test_shed_against_deprecated_original_rejected` (AC-05 negative, R-10 #2) — REQUIRED
- **Act**: `context_edge remove` with `source_id = A.id` (Deprecated post-correct).
- **Assert**: rejected as **frozen-source** (Active-source requirement). This is by design
  (SR-08) — proving shed must target B, not a carry-forward bug.

---

## AC-09 — no outgoing ceiling

### `test_correct_no_ceiling_all_carry_above_50` (AC-09) — REQUIRED
- **Arrange**: seed A with **> `REDIRECT_CEILING` (50)** eligible outgoing edges (e.g. 60).
- **Act**: `context_correct(A→B)`.
- **Assert**: **all** 60 carry (60 rows with `source_id = B.id`); `edges_carried == 60`; no
  truncation; **no ceiling warn** fired. Couples to AC-04: ineligible classes still excluded.
  Contrast with the incoming redirect path, which *does* cap at 50 — the outgoing path has no
  cap by design (eligibility is the sole bound).

---

## infra-001 MCP mirror (Stage 3c — see OVERVIEW.md harness plan)
- `test_correct_response_includes_edges_carried` (tools suite) — AC-11a through MCP.
- `test_correct_omits_edges_carried_when_zero` (tools suite) — AC-11b through MCP.
- `test_correction_carries_outgoing_edges_visible_on_new_entry` (lifecycle suite, depth-1
  read, no tick) — AC-01/AC-02 through MCP.

## Notes
- `#[tokio::test]`, Arrange/Act/Assert, deterministic. Extend `open_store_and_insert_active`,
  `insert_edge`, and the vnc-015 `edges`-param correction fixtures — no isolated scaffolding.
- Carry-loop internals (count contract, Contradicts, fault injection, created_at) are unit-
  tested in **run_carry_forward_loop.md**; this plan asserts the handler-observable behavior.
