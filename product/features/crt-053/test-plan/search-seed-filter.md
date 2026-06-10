# crt-053 Test Plan — Component: `SearchService::search` seed filter

**Production component**: `crates/unimatrix-server/src/services/search.rs`,
Phase 0 `seed_ids` build inside `if self.ppr_expander_enabled` (~`:915`).
**Pseudocode**: `pseudocode/search-seed-filter.md`.
**Test surface (OQ-1, BINDING)**: Rust full-pipeline integration —
`crates/unimatrix-server/tests/pipeline_e2e.rs` (live `SearchService::search` via `TestHarness`).
**Production delta under test**:
`.filter(|(e, _)| e.status == Status::Active)` on the `seed_ids` collection — nothing else.

> Discipline (binding): ID-level behavior only, never penalty constants (C-04). No eval-harness
> gate (NFR-05). Every absence arm is paired with a control arm (R-04). NO deprecated-absence-in-
> Flexible assertion (ANTI-AC-01). Verify by neighbor-ID presence/absence, never by inspecting a
> `Direction::` enum (SR-06).

---

## Test-Infrastructure Prerequisite (Stage 3b/3c, `test_support.rs`)

The current `TestHarness::new()` wires `InferenceConfig::default()` →
`ppr_expander_enabled = false`. AC-01/AC-04/AC-05 require the expander **ON**. Add a cumulative,
non-isolated constructor variant:

```rust
// test_support.rs — extend, do not duplicate the harness.
// Threads a caller-supplied InferenceConfig (or an enabled flag) into
// ServiceLayer::with_rate_config in place of the hardcoded default().
pub async fn new_with_expander(store_path: &Path, ppr_expander_enabled: bool) -> Option<Self>
```

This is **test support only** — not a production edit; it does NOT count against C-01.
A positive-edge authoring helper already exists:
`TestHarness::insert_graph_edge(source_id, target_id, relation_type)` +
`TestHarness::rebuild_typed_graph()`. Use `relation_type = "RelatedTo"` (a positive, traversable
edge type per `graph_expand.rs:50` — `CoAccess|Supports|Informs|Prerequisite|RelatedTo`).

---

## Fixture topology (shared by AC-01, AC-05 and their control arms)

Author a pool where a deprecated seed's neighbor is reachable by NO active path (R-04/R-07):

```
B (Active)      --RelatedTo-->  Y        # Y reachable ONLY via active seed B
A (Deprecated)  --RelatedTo-->  X        # X reachable ONLY via deprecated seed A
```

Preconditions to ASSERT IN THE TEST (so the control arm is not vacuous — R-04 scenario #2):
- A's positive out-edge A→X exists in the typed graph after `rebuild_typed_graph()`.
- X has NO incoming positive edge from any Active entry (reachable only forward from A).
- Y has NO incoming positive edge from any Deprecated entry (reachable only forward from B).
- A, B, X, Y all surface enough HNSW similarity to the query that, if injected, they would appear in
  `k` — i.e. the only reason X is absent is the seed filter, not similarity floor. (Choose contents
  with verbatim-overlapping terms to the query, per #724 deterministic-similarity construction.)

Edge directions are stated concretely (forward BFS: seed S with edge S→Z surfaces Z, `graph_expand`
doc:27). Verification is by neighbor-ID outcome only.

---

## AC-01 — Seed filter excludes deprecated-only neighbor (with control arm)

**Maps**: SCOPE Validation #1, SR-05, R-02 (positive arm), R-04 (control arm), R-07 (direction).

`test_seed_filter_excludes_deprecated_only_neighbor` (expander ON):
- **Arrange**: `TestHarness::new_with_expander(path, true)`. Insert B(Active), A(Deprecated),
  X(Active), Y(Active). `insert_graph_edge(B, Y, "RelatedTo")`,
  `insert_graph_edge(A, X, "RelatedTo")`. `rebuild_embeddings(&[B,A,X,Y])`, `rebuild_typed_graph()`.
- **Act**: `harness.search(query, k=10)`.
- **Assert (positive retention, R-02)**: result IDs CONTAIN `Y` — the active seed B's neighbor IS
  injected. `assert!(ids.contains(&Y))`.
- **Assert (filter effect)**: result IDs DO NOT CONTAIN `X` — the deprecated seed A's neighbor is
  NOT injected. `assert!(!ids.contains(&X))`.
- **Assert (precondition, anti-vacuous)**: confirm via `harness.call_graph(...)` or a direct typed-
  graph read that edge A→X exists and X has no active in-edge — so absence is filter-caused, not
  unreachability.
- **NOTE**: A itself (Deprecated) MAY still appear in results from the HNSW path — that is correct
  (C-03). Do NOT assert A absent (ANTI-AC-01). The assertion is about the *neighbor* X.

`test_seed_filter_excludes_deprecated_only_neighbor_control` (R-04 control arm — REQUIRED):
- **Arrange**: identical fixture and edges, but force A to `Status::Active`
  (`update_status(A, Status::Active)` then `rebuild_*`). Expander still ON.
- **Act**: `harness.search(query, k=10)`.
- **Assert**: result IDs CONTAIN `X`. `assert!(ids.contains(&X))`.
  → X reappears the moment A is an eligible (active) seed, **proving X's absence in the real arm is
  caused by the active-only filter, not by X being unreachable** (#4902). Y still present too.

> Equivalent acceptable control form (brief permits either): keep A `Deprecated` but run the second
> arm against a harness whose filter is absent (pre-crt-053 seed build). Forcing A active is preferred
> because it needs no second code path. State the chosen form in RISK-COVERAGE-REPORT.

---

## AC-05 — Supersession false-positive guard (with control arm)

**Maps**: SCOPE Validation #1 / SR-05; supersession variant of AC-01; R-04 control arm.

Topology: Deprecated **A** `superseded_by` Active **B**; both carry a positive out-edge
(A→X, B→Y) in ADDITION to the supersession relation.

`test_supersession_false_positive_guard` (expander ON):
- **Arrange**: B(Active), A(Deprecated, `superseded_by = [B]`), X(Active), Y(Active).
  `insert_graph_edge(A, X, "RelatedTo")`, `insert_graph_edge(B, Y, "RelatedTo")`. Set A's
  `superseded_by` via the store record (as `test_supersession_injection` does today). Rebuild.
- **Act**: `harness.search(query, k=10)`.
- **Assert**: `ids.contains(&Y)` (BFS expands from B's path) AND `!ids.contains(&X)`
  (NOT from A's path).
- **Assert (precondition)**: A→X edge present; X reachable by no active path. (Anti-vacuous.)
- **Do NOT** assert A absent from results (ANTI-AC-01).

`test_supersession_false_positive_guard_control` (R-04 control arm — REQUIRED):
- **Arrange**: identical, but force A `Status::Active` (the supersession topology stays, only the
  lifecycle status flips — mirrors the `superseded_active.toml` conflict case). Expander ON. Rebuild.
- **Act**: search.
- **Assert**: `ids.contains(&X)` — X reappears once A is an active seed → A's neighbor's absence in
  the real arm is filter-caused. (#4902.)

> R-10 disposition: if this supersession fixture reproduces #406 (multi-hop terminal-active redirect
> "failing"), RAISE as a fixture-divergence signal vs ass-073's eval graph — do NOT patch retrieval.

---

## AC-04 — Terminal-active heads survive the filter (positive retention)

**Maps**: SR-02, FR-03, R-02, R-09 (6b head reaches the seed set, no unfiltered bypass).

`test_seed_filter_retains_terminal_active_head` (expander ON):
- **Arrange**: a superseded chain whose terminal is an Active head H that gets 6b-injected
  (reuse the `test_supersession_injection` pattern: original O Deprecated `superseded_by` H Active).
  Give H a positive out-edge `insert_graph_edge(H, Z, "RelatedTo")` to an Active Z reachable only
  via H. Rebuild.
- **Act**: `harness.search(query, k=10)`.
- **Assert (RETAIN, not drop)**: `ids.contains(&H)` — the 6b terminal-active head survived the
  filter and is in results; AND `ids.contains(&Z)` — H anchored the walk and its neighbor Z IS
  injected. `assert!(ids.contains(&H) && ids.contains(&Z))`.
- This is the R-02 over-drop guard: proves the filter retains legitimate active anchors (6b heads
  pass by construction, `:814` guard) and does not silently shrink expansion.

---

## AC-02 — Off-path bit-for-bit identical (expander OFF)

**Maps**: SCOPE Validation #2, C-02, FR-07, NFR-01, R-05.

`test_off_path_identical_to_baseline` (expander OFF):
- **Arrange**: `TestHarness::new(path)` (default → expander OFF). Use a fixture containing
  deprecated + active entries WITH the same positive edges as AC-01.
- **Act**: `harness.search(query, k=10)`.
- **Assert**: result set, ORDER, and scores are identical to the pre-crt-053 baseline for the same
  fixture/query. Concretely: no neighbor injection occurs (X and Y absent unless independently HNSW-
  surfaced), and the ordered `(id, final_score)` sequence matches the baseline. The simplest robust
  form: assert NO graph-expanded neighbor appears (since the whole Phase 0 block is skipped) and the
  existing default-off pipeline tests pass UNCHANGED.
- **Review companion (R-05)**: confirm the new `.filter(...)` is lexically inside the
  `if self.ppr_expander_enabled` block and the `seed_ids` binding is referenced nowhere else — so the
  OFF path never evaluates the filter (structural, not disciplinary).

> Because crt-053 only narrows `seed_ids` inside the enabled branch, the cleanest AC-02 evidence is
> that ALL pre-existing expander-OFF tests in `pipeline_e2e.rs` + the `search.rs` unit suite remain
> green with zero edits. Add one explicit "no injection when expander off" assertion to make the
> guarantee discoverable.

---

## AC-03 — HNSW ranking path unchanged (deprecated present + penalized in Flexible)

**Maps**: SCOPE Validation #3, C-01, C-03, FR-06, R-06 (anti-AC presence side).

- **Existing-suite gate**: the entire `search.rs` `mod tests` penalty/utility/ranking suite and the
  existing `pipeline_e2e.rs::test_active_above_deprecated` MUST pass with **zero modification**.
  (If any existing test needs editing to pass, that is a scope-creep / regression signal — STOP and
  raise, do not edit. GATE-02 / #4495.)
- **Positive presence-of-deprecated-in-Flexible assertion** (R-06, ANTI-AC-01 positive side):
  in a Flexible search where a Deprecated entry is HNSW-similar to the query, ASSERT the deprecated
  entry IS PRESENT in results and is RANKED BELOW a comparable active (relative rank, not score
  value — #724). `test_active_above_deprecated` already encodes this; confirm it still holds. Add a
  one-line presence assert if not already explicit: `assert!(ids.contains(&deprecated_id))`.
- **FORBIDDEN (ANTI-AC-01)**: no assertion that a deprecated entry is ABSENT from Flexible results.

---

## Edge-case tests (RISK-TEST-STRATEGY §Edge Cases)

| Test (planned) | Setup | Assert | Risk |
|----------------|-------|--------|------|
| `test_all_seeds_deprecated_no_panic` | all candidates Deprecated, expander ON, A→X edge | no panic; X NOT injected; HNSW results still returned (non-empty) | empty-seed boundary |
| `test_no_deprecated_seeds_is_noop` | all-active fixture, expander ON | injected set identical to the same fixture's unfiltered behavior (filter is a no-op) | parity |
| `test_superseded_but_active_is_retained` | entry S `status=Active`, `superseded_by=[H]`, edge S→W | `ids.contains(&W)` — S still anchors (discriminator is status, not `superseded_by`) | common misread |
| `test_proposed_or_quarantined_seed_excluded` | seed P `status=Proposed` (and/or Quarantined), edge P→V, expander ON | `!ids.contains(&V)` — a non-Deprecated non-Active status is dropped → proves predicate is `== Active`, not `!= Deprecated` | R-12/FR-02/GATE-05 |

**Explicitly NOT a test** (documented to prevent a tester writing a failing one): a 6b head whose
neighbor is reachable only via the vnc-017 >50-edge redirect ceiling is knowingly NOT redirected
(Locked Decision 4/5). Assert nothing about it.

---

## Quarantine gate (R-11 / GATE-03)

- `test_quarantined_still_excluded_by_enforcement` (or confirm an existing equivalent):
  a Quarantined entry is excluded from results by the `:950` enforcement path, INDEPENDENT of the new
  seed predicate. Assert the Quarantined entry is absent from results AND confirm by diff review that
  `search.rs:950` (`SecurityGateway::is_quarantined`) is UNCHANGED. The seed predicate dropping
  Quarantined *seeds* is defense-in-depth, not a replacement for `:950`.

---

## Review / grep gates (Stage 3c — not feature ACs, executed by tester + validator)

| Gate | Method | Pass condition |
|------|--------|----------------|
| GATE-01 | diff review | Production diff touches only the `seed_ids` build in `search.rs` (+ a `Status` import if needed). No other production line changed. |
| GATE-02 | `Grep` | Existing `graph_expand` write-only negative tests UNCHANGED; no existing penalty/ranking test edited (#4495). |
| GATE-03 | diff review | `search.rs:950` quarantine enforcement unchanged. |
| GATE-04 | `Grep` over crt-053 test files | ZERO `p_at_5` / `mrr` / `soft_gt` / eval-harness scoring pass/fail gate used as acceptance. |
| GATE-05 | source `Grep` | Predicate is `e.status == Status::Active` (typed enum), not a string compare; `test_proposed_or_quarantined_seed_excluded` proves `== Active`. |
| ANTI-AC-01 | `Grep` over crt-053 test files | NO assertion that a deprecated entry is absent from Flexible/search results. |

---

## Test naming summary (for Stage 3b/3c implementation)

```
test_seed_filter_excludes_deprecated_only_neighbor              # AC-01 real
test_seed_filter_excludes_deprecated_only_neighbor_control      # AC-01 control (R-04)
test_supersession_false_positive_guard                          # AC-05 real
test_supersession_false_positive_guard_control                 # AC-05 control (R-04)
test_seed_filter_retains_terminal_active_head                   # AC-04 (R-02 retention)
test_off_path_identical_to_baseline                            # AC-02 (C-02/R-05)
# AC-03: existing suite green + presence-of-deprecated-in-Flexible (test_active_above_deprecated)
test_all_seeds_deprecated_no_panic                             # edge
test_no_deprecated_seeds_is_noop                               # edge
test_superseded_but_active_is_retained                        # edge
test_proposed_or_quarantined_seed_excluded                    # edge / R-12 / GATE-05
test_quarantined_still_excluded_by_enforcement                # R-11 / GATE-03
```
