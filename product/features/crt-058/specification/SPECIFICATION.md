# crt-058 — Specification: Eager Inbound/Outbound Edge Cleanup at Deprecation Time

Source scope: `product/features/crt-058/SCOPE.md`
Scope risk assessment: `product/features/crt-058/SCOPE-RISK-ASSESSMENT.md`
Tracking: GitHub Issue #895 (enhancement)

## Objective

When `context_deprecate` retires an entry (a terminal flip to `Deprecated` with no successor), agent/human-authored graph edges touching that entry become live dangling references to retired knowledge. This feature synchronously deletes those agent-authored edges (both directions, `source = 'agent'`) at the deprecation event — pulling the `EveryTick` orphaned-edge compaction's blanket delete forward for the single entry being deprecated — reports the removed-edge count inline to the caller, and records the removal in the audit log. It is deterministic graph maintenance: non-fatal, additive, and a strict subset of what the tick would remove.

## Domain Models

Definitions of ubiquitous terms used throughout this specification and downstream artifacts.

- **Entry** — a knowledge item row in `entries`, carrying a `status` (`Active` / `Deprecated` / others). The subject of `context_deprecate`.
- **Deprecation (bare)** — the terminal status flip to `Deprecated` via `context_deprecate` with `superseded_by` remaining NULL. Distinct from **Correction** (`context_correct`), which sets a successor and repoints edges rather than deleting them.
- **Graph edge** — a row in `graph_edges` with `(source_id, target_id, relation_type, source, created_by, ...)`. Directional.
- **Inbound edge** (relative to an entry E) — an edge with `target_id = E`: another entry points at E.
- **Outbound edge** (relative to an entry E) — an edge with `source_id = E`: E points at another entry.
- **Provenance (`source` column)** — the origin classifier on `graph_edges.source`. Takes exactly: `agent` (`EDGE_SOURCE_AGENT`; every agent/human-directed edge write), and the machine generators `nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8`. There is no distinct `human` value in this column.
- **Agent/human-authored edge** — an edge with `source = 'agent'`. The single provenance value the eager delete targets.
- **Machine-generated edge** — an edge with `source` in {`nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8`}. Disposable derived data; NOT eagerly deleted (left to the tick).
- **Eager delete** — the new synchronous statement added by this feature at `context_deprecate`.
- **Tick / EveryTick compaction** — `run_orphaned_edge_compaction` (`background.rs:805`), Phase 2 blanket delete `DELETE FROM graph_edges WHERE source_id NOT IN (Active) OR target_id NOT IN (Active)`. Runs ~every 900s over all sources. The backstop.
- **eager ⊆ tick** — the load-bearing invariant: the set of edges the eager delete removes for a deprecated entry is a strict subset of the set the tick would remove for that same (now non-Active) entry. The eager path can only ever do a subset of the backstop's work, earlier.
- **`edges_removed`** — an `Option<u64>` count of rows deleted by the eager statement (`rows_affected()`), threaded to the caller and audit record. The `Option` encodes ran-vs-failed: `None` = the delete failed or did not run (advisory omitted/suppressed); `Some(n)` = the delete ran and removed `n` edges, including `Some(0)` when it ran and found nothing (advisory renders a literal `0`).

## Functional Requirements

Each requirement is individually testable.

- **FR-01 — Eager delete of agent-authored edges, both directions.** At `context_deprecate`, after the entry is flipped to `Deprecated`, the handler executes a single delete removing every `graph_edges` row where the entry is either endpoint and provenance is agent-authored: `(source_id = ?entry OR target_id = ?entry) AND source = 'agent'`. This removes inbound (`target_id = entry`) and outbound (`source_id = entry`) agent-authored edges. Verification: seed inbound and outbound `source='agent'` edges on an entry, deprecate it, assert both are gone from `graph_edges`.

- **FR-02 — Machine-generated edges untouched by the eager path.** The eager delete removes only `source = 'agent'` rows. Edges with `source` in {`nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8`} touching the entry remain in `graph_edges` after `context_deprecate` returns. No relation-type filtering is applied within the agent-authored set (`Prerequisite`, `Supports`, `Contradicts`, etc. are all removed if `source='agent'`). Verification: see AC-04 per-source matrix.

- **FR-03 — Inline count to caller.** The `context_deprecate` response surfaces the `edges_removed` count as an `Option<u64>` advisory threaded through `format_deprecate_success` → `format_status_change` (`mcp/response/mutations.rs`), additive and backward-compatible across all three formats (Summary / Markdown / Json). Per ADR-004: `Some(n)` renders a literal `n` (including `Some(0)` when the delete ran and removed nothing); `None` (delete failed / did not run) omits/suppresses the advisory. Verification: see AC-04 per-format matrix and AC-05 zero case.

- **FR-04 — Audit record of the removal.** The removal is emitted as an `AuditEvent` via the already-wired audit path (`audit_fire_and_forget` / detached spawn in `change_status_with_audit`), recording at minimum the deprecated entry id and the removed-edge count. Verification: deprecate an entry with N agent edges, assert an audit record exists carrying entry id and count = N.

- **FR-05 — Non-fatal.** A failure in the eager delete never propagates into the `context_deprecate` result. On any delete error the handler logs at `warn`, treats the count as zero / omits the advisory, and returns the normal deprecation success. The status flip and the deprecation response are unaffected. Verification: see AC-06.

- **FR-06 — Synchronous completion.** The eager delete completes before `context_deprecate` returns. No async / deferred task is used that could leave the edges present after the response. Verification: on return from `context_deprecate`, the agent-authored edges are already absent (AC-09).

- **FR-07 — Idempotent path performs no delete.** The eager delete is placed after the handler's step-5 already-`Deprecated` early-return (`tools.rs:~1442`). Re-deprecating an already-`Deprecated` entry returns via the early-return and performs no delete and reports no removal. Verification: see AC-07.

- **FR-08 — No new persistence or tick-path change.** No new table, schema migration, prune lifecycle, or `EveryTick` compaction change is introduced. The delete reuses the existing `graph_edges` write path, `write_pool_server()`, and existing indexes (`idx_graph_edges_source_id`, `idx_graph_edges_target_id`). Verification: schema/migration count unchanged; compaction code unchanged; the statement uses `write_pool_server()`.

- **FR-09 — eager ⊆ tick (subset invariant).** For any entry deprecated via `context_deprecate`, every edge removed by the eager delete is also an edge the `EveryTick` compaction would remove for that entry once it is non-Active. The eager predicate (`(source_id=?entry OR target_id=?entry) AND source='agent'`) must never remove an edge the tick would keep or repoint. In particular, the eager path must never delete an edge that the tick's Phase 1 would repoint to a successor. Verification: see AC-10 (behavioral subset test).

## Non-Functional Requirements

- **NFR-01 — Single indexed statement.** The eager delete is one SQL statement served by `idx_graph_edges_source_id` + `idx_graph_edges_target_id` (SQLite OR-by-index-union). No per-edge loop, no full-table scan. Constraint, not a benchmark: the statement text is a single `DELETE ... WHERE (source_id = ? OR target_id = ?) AND source = ?`.

- **NFR-02 — Write pool.** The delete executes on `write_pool_server()` — the pool used by the compaction DELETE and `delete_graph_edge`. No read pool.

- **NFR-03 — Bounded latency contribution.** The synchronous delete adds one indexed write to the `context_deprecate` critical path. Its cost is O(edges touching the entry), bounded by that entry's degree; it introduces no scan proportional to total `graph_edges` size.

- **NFR-04 — Backward compatibility.** The `edges_removed` advisory is additive: existing `context_deprecate` callers and response parsers that ignore the advisory slot are unaffected. Per ADR-004, the `Option<u64>` encodes ran-vs-failed: a successful delete that removed nothing renders `Some(0)` (a literal `0`), which is unambiguous; only `None` (delete failed / did not run) omits the advisory. The rendered `0` is a new but additive advisory line and does not alter the pre-existing deprecation success fields.

- **NFR-05 — Log discipline.** The non-fatal failure `warn` log is a real diagnostic signal, not an expected-suppressed error — it must be emitted and visible (not silently swallowed at a filtered level). Aligns with fire-and-forget log discipline (lesson #3448 class).

- **NFR-06 — Provenance-enumeration-bound completeness.** Eager completeness is bound to the current `graph_edges.source` enumeration. `source = 'agent'` is an inclusive single-value filter; if a future `EDGE_SOURCE_*` provenance for agent/human-authored edges ships, the eager path would miss it. This is acceptable only because the miss is subset-safe: the tick backstop still removes it. This coupling is documented, not defended against in code.

## Acceptance Criteria

Each maps to SCOPE.md acceptance criteria and the folded-in scope risks. Verification methods are behavioral/state-based, not call-count or string-presence (SR-04 discipline).

- **AC-01 (SCOPE AC-01, FR-01) — Both-direction agent-edge removal.**
  Given an Active entry E with agent-authored inbound edges (other entries → E) and outbound edges (E → other entries), when E is deprecated via `context_deprecate`, then all those `source='agent'` edges are absent from `graph_edges` when the call returns.
  Verify: seed ≥1 inbound and ≥1 outbound `source='agent'` edge; deprecate; query `graph_edges` by both `target_id=E` and `source_id=E` filtered `source='agent'` → zero rows.

- **AC-02 (SCOPE AC-02, FR-03) — Inline count reported.**
  The `context_deprecate` response includes the count of edges removed.
  Verify: as part of the AC-04 per-format matrix.

- **AC-03 (SCOPE AC-03, FR-04, SR-01, ADR-002) — Audit record content.**
  The removal is recorded in the audit log with the deprecated entry id, the removed-edge count, and the removed-edge tuples (per ADR-002; see AC-11).
  Verify: deprecate an entry with N agent edges; read back the audit record; assert it carries entry id = E, count = N, and the N removed-edge tuples (AC-11).

- **AC-04 (SCOPE AC-04, FR-02/FR-03, SR-03/SR-04) — Per-source and per-format behavioral matrix.**
  Two matrices, both state/behavior-based:
  - **Per-source removal matrix.** Seed exactly one edge of each `source` value touching E: `agent`, `nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8`. Deprecate E. Assert `agent` is removed and each machine source remains present in `graph_edges`. (This also proves FR-02 and roots SR-03's enumeration test in state.)
  - **Per-format count matrix.** For a deprecation that removes N>0 agent edges, render each of the three response formats (Summary, Markdown, Json) and assert the `edges_removed` count value = N is present and correct in each. The Json assertion parses the structured field and compares the integer (not a substring match); Summary and Markdown assert the rendered count value, not merely that a call was made. This detects a format that drops the threaded argument and ships green (SR-04 / #5427).

- **AC-05 (SCOPE AC-05, FR-03/NFR-04, ADR-004) — Zero case renders `Some(0)`.**
  Deprecating an entry with no agent-authored edges runs the eager delete, removes nothing, and reports `Some(0)` — a literal `0` edges-removed in all three formats (Summary / Markdown / Json). The advisory is rendered, NOT omitted; omission (`None`) is reserved for the delete-failed/did-not-run case (AC-06). Deprecation success fields are otherwise unchanged.
  Verify: deprecate an entry with zero `source='agent'` edges (machine edges may be present and must remain); assert `edges_removed = Some(0)`; assert each of the three formats renders a literal `0` count (Json parses the integer field = 0; Summary/Markdown assert the rendered `0`); assert deprecation success is otherwise unchanged.

- **AC-06 (SCOPE AC-06, FR-05, SR-05) — Non-fatal on failure.**
  When the eager delete fails, `context_deprecate` still returns its normal success result; the failure is logged at `warn` (not propagated); the status flip stands; and the `EveryTick` compaction still removes the edges on a subsequent pass.
  Verify: inject a delete failure (fault-injected or forced error path); assert `context_deprecate` returns success, the entry is `Deprecated`, a `warn` log was emitted, `edges_removed = None` so the advisory is omitted/suppressed in all three formats (distinct from the `Some(0)` zero case of AC-05), and the agent edges are still present (to be swept by the tick). Then run the compaction and assert the edges are removed by the backstop.

- **AC-07 (SCOPE AC-07, FR-07, SR-06) — Idempotency.**
  Re-deprecating an already-`Deprecated` entry performs no delete and reports no removal.
  Verify: deprecate E (first call may remove edges); seed a fresh `source='agent'` edge touching E; deprecate E again; assert the second call returns via the early-return, the freshly-seeded edge is untouched by the eager path, and no new audit removal record is written for the second call.

- **AC-08 (SCOPE AC-08, FR-08) — No new persistence / no tick change.**
  No new table, schema migration, or `EveryTick` compaction change is introduced; the delete reuses `graph_edges`, `write_pool_server()`, and existing indexes.
  Verify: schema version and migration list unchanged; `run_orphaned_edge_compaction` unchanged; the new statement targets `graph_edges` on `write_pool_server()`.

- **AC-09 (SCOPE AC-09, FR-06) — Synchronous before response.**
  The edges are absent immediately upon return from `context_deprecate` — no async leaves them present after the response.
  Verify: immediately after the `context_deprecate` call returns (no tick, no sleep), query `graph_edges` and assert the agent-authored edges are already gone.

- **AC-10 (SR-02, FR-09) — eager ⊆ tick subset invariant (behavioral).**
  A test that fails if the eager delete removes an edge the tick would not remove (or would repoint) for a non-Active entry. Concretely: construct a scenario where the tick's Phase 1 repoint would rescue an edge — i.e. a successor-bearing target — and prove the bare-deprecation eager path never reaches that scenario, and that for the deprecated (non-Active, no-successor) entry the eager-removed set is a strict subset of the tick-removed set.
  Verify (behavioral, state-based): 
  1. Seed entry E deprecated via bare `context_deprecate` with agent + machine edges. Capture the eager-removed set S_eager.
  2. In an equivalent fixture, deprecate E the same way but do NOT run the eager delete; run the `EveryTick` compaction against the non-Active E and capture the tick-removed set S_tick.
  3. Assert S_eager ⊆ S_tick (every eagerly-removed edge is also tick-removed) and S_eager ⊂ S_tick where machine edges exist (strict subset).
  4. Separately seed a successor-bearing correction scenario (`context_correct`, `superseded_by` set) and assert the eager delete is never invoked on that path (chokepoint-only, FR excludes `correct_entry`), so no repointable edge is ever eagerly deleted.
  The test must fail if the eager predicate is widened to remove an edge the tick would keep or repoint.

- **AC-11 (SR-01, ADR-002) — Removed-edge tuples in audit (firm).**
  The audit record for a deprecation MUST carry the removed-edge tuples `(source_id, target_id, relation_type)` for every eagerly-removed edge, for reconstructability of a wrongful eager delete. Count-only auditing is not sufficient.
  Verify: deprecate an entry with N agent-authored edges of known `(source_id, target_id, relation_type)`; read back the audit record; assert it contains exactly the N tuples matching the actual removed rows (set equality against the pre-delete edge set), not merely a count.

## User / Agent Workflows

- **Deprecate with dangling agent references.** An agent calls `context_deprecate(id)` on an Active entry that other entries reference via agent-authored `Prerequisite`/`Supports`/etc. edges. The entry flips to `Deprecated`; the eager delete removes all agent-authored inbound and outbound edges synchronously; the response tells the agent "N edges removed"; the audit log records it. Downstream retrieval no longer follows dangling references to the retired entry.

- **Deprecate with no agent references.** An agent deprecates an entry with only machine-generated edges (or none). The flip succeeds, `edges_removed = 0`, no advisory shown, machine edges left for the tick.

- **Re-deprecate.** An agent (or retry) calls `context_deprecate` on an already-`Deprecated` entry. The idempotency early-return short-circuits; no delete, no advisory, no audit removal.

- **Delete failure (degraded).** The eager delete fails transiently. The agent still gets a successful deprecation; a `warn` is logged for operators; the tick sweeps the edges within ≤900s.

## Constraints

Technical constraints inherited from SCOPE.md (all binding on the architect/implementer):

- **C-01 Non-fatal** — the eager delete must never propagate an error into the `context_deprecate` result (mirror `confidence.recompute` / audit fire-and-forget). Backstop is the tick.
- **C-02 After idempotency early-return** — placed past the step-5 already-`Deprecated` guard (`tools.rs:~1442`).
- **C-03 After the status flip** — the delete predicate keys on the entry id being non-Active; order is flip (`deprecate_with_audit`) → delete → format.
- **C-04 Synchronous** — edges gone before the response returns; no async deferral.
- **C-05 Write pool** — `write_pool_server()` only.
- **C-06 Single indexed statement, both directions, agent-authored only** — one `DELETE FROM graph_edges WHERE (source_id = ?entry OR target_id = ?entry) AND source = 'agent'`; filter on the `source` column (F2 discipline, `background.rs:849`), never a relation-type blocklist; no per-edge loop.
- **C-07 eager ⊆ tick** — the eager filter must remain a strict subset of the tick's status predicate. Load-bearing (bugfix-458 / #3910; lesson #5417). If the tick predicate ever changes (gains a source filter, repoints agent edges), the subset must be re-verified.
- **C-08 No new helper indirection required** — add the statement inline or as a small `edge_write.rs` function beside `delete_graph_edge`, reusing its `write_pool_server()` pattern.
- **C-09 Response surface** — add an optional `edges_removed` advisory to `format_status_change`, additive and backward-compatible across Summary/Markdown/Json.
- **C-10 Chokepoint-only** — bare deprecation via `context_deprecate`; `correct_entry` excluded (already repoints inbound via `repoint_deprecated_target_edges`).
- **C-11 Compaction-as-backstop is a standing dependency invariant** — correctness of the non-fatal design depends on the `EveryTick` compaction continuing to sweep non-Active endpoints over all sources. Any future change to the compaction must re-check this backstop guarantee (SR-05).

## Dependencies

Existing components reused (no new external dependencies):

- **`graph_edges`** table — the write target; columns `source_id`, `target_id`, `relation_type`, `source`.
- **`idx_graph_edges_source_id`** (`db.rs:969`), **`idx_graph_edges_target_id`** (`db.rs:972`) — cover the OR predicate.
- **`write_pool_server()`** — the write pool.
- **`context_deprecate` handler** (`tools.rs:1413`) — insertion site: after step-5 idempotency early-return (`~1442`), after `deprecate_with_audit`, before `format_deprecate_success`.
- **`deprecate_with_audit`** (`server.rs:949`) → **`change_status_with_audit`** (`server.rs:1089`) — the bare-flip path.
- **`delete_graph_edge`** (`edge_write.rs:244`) — pattern reference for a new by-endpoint delete function; `EDGE_SOURCE_AGENT` constant (`edge_write.rs:28`).
- **Audit path** — `audit_fire_and_forget` (`server.rs:650`), detached spawn in `change_status_with_audit` (`server.rs:~1163`); `AuditEvent`.
- **`format_deprecate_success`** → **`format_status_change`** (`mcp/response/mutations.rs:16`) — response threading.
- **`run_orphaned_edge_compaction`** (`background.rs:805`) — the backstop (unchanged; referenced by AC-06, AC-10, C-11).

## NOT in Scope

Explicit exclusions (scope additions are variances the vision guardian flags):

- **Not self-learning / not drift-adaptation.** Deterministic graph maintenance; feeds no model, confidence, or adaptation path.
- **Not detection / not a governance nudge.** No interpretation, validation, edge-semantics filtering, or capped referrer list. The condition is resolved (deleted), not flagged.
- **No relation-type filtering.** Agent-authored edges removed regardless of relation type; provenance-filtered only.
- **System/machine edges not eagerly deleted.** `nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8` left to the tick.
- **No revival of `DependencyOnDeprecatedRule` or the cohesion metric** (retired #891). No `context_cycle_review` metric, no findings table.
- **`context_correct` / successor path excluded.** Correction already repoints inbound edges.
- **No change to the EveryTick compaction.** It stays the unchanged backstop.
- **CoAccess cold-start migration** — out of scope (accepted; separate future issue).
- **No new table, schema migration, or prune lifecycle.**
- **No soft-delete / undo of eagerly-removed edges** beyond whatever audit reconstructability the architect elects (AC-11). The delete is irreversible by design.

## Key Decisions Made (Requirement Interpretations)

- **AC-10 split into a positive subset assertion and a negative chokepoint assertion.** SR-02 asks for a test that fails if the eager delete removes an edge the tick would not. Expressed both as a direct S_eager ⊆ S_tick state comparison and as a chokepoint-exclusion assertion (successor/repoint scenario never reaches the eager path), because the repoint-rescue case only exists on the correction path.
- **AC-04 as two behavioral matrices (per-source, per-format).** Folds SR-03 (per-source enumeration) and SR-04 (per-format count threading) into state/parse-based assertions; explicitly rejects call-count/substring tests.
- **Zero case (AC-05) renders `Some(0)` (ADR-004, human-resolved at design-review gate).** The `Option<u64>` advisory encodes ran-vs-failed: a successful delete that removed nothing renders a literal `0`; only `None` (delete failed / did not run) omits the advisory. Rendering `Some(0)` is unambiguous where omission is not.
- **AC-11 firm — tuple-level auditing (ADR-002, human-resolved at design-review gate).** The audit records the removed-edge tuples `(source_id, target_id, relation_type)`, not just entry id + count, for reconstructability (SR-01). Promoted from conditional to a required acceptance criterion.

## Resolved at Design-Review Gate

1. **Audit granularity (AC-11 / SR-01) — RESOLVED (ADR-002).** Tuple-level auditing accepted: the audit records the removed-edge tuples `(source_id, target_id, relation_type)`, not just entry id + count. AC-11 is firm.
2. **Zero-case advisory rendering (AC-05 / NFR-04) — RESOLVED (ADR-004).** Render `Some(0)` — a literal `0` in all three formats when the delete ran and removed nothing. `None` (delete failed / did not run) is the omit case.

## Open Questions (for Architect)

1. **New `edge_write.rs` function vs inline statement (C-08).** Scope permits either; architect to pin placement (a small by-endpoint delete beside `delete_graph_edge` vs inline in the handler) and confirm no interaction with step-7 `confidence.recompute` ordering (SR-06).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #3910 (all cleanup passes on the same secondary table must use identical status filters — the eager ⊆ tick basis), #5431 (retired DependencyOnDeprecatedRule read stale Prerequisite pairs — the condition this feature resolves), #4425 (EDGE_SOURCE_AGENT constant + created_by convention), #3883 (use write_pool_server() for background graph_edges writes). No novel generalizable pattern to store; spec decisions are feature-specific.
