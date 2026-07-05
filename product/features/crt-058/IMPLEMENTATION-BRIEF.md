# crt-058 Implementation Brief — Eager Agent-Authored Edge Cleanup at `context_deprecate`

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-058/SCOPE.md |
| Scope Risk Assessment | product/features/crt-058/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/crt-058/specification/SPECIFICATION.md |
| Architecture | product/features/crt-058/architecture/ARCHITECTURE.md |
| Risk / Test Strategy | product/features/crt-058/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-058/ALIGNMENT-REPORT.md |
| ADR-001 (eager delete + tick backstop) | product/features/crt-058/architecture/ADR-001-eager-delete-at-deprecation-source.md |
| ADR-002 (audit removed-edge tuples) | product/features/crt-058/architecture/ADR-002-audit-removed-edge-tuples.md |
| ADR-003 (eager ⊆ tick executable invariant) | product/features/crt-058/architecture/ADR-003-eager-subset-tick-invariant.md |
| ADR-004 (`edges_removed` response plumbing) | product/features/crt-058/architecture/ADR-004-edges-removed-response-plumbing.md |

## Goal

When `context_deprecate` retires an entry (a terminal `Active → Deprecated` flip with no successor), synchronously delete the agent-authored (`source = 'agent'`) `graph_edges` rows touching that entry in both directions — pulling the `EveryTick` orphaned-edge compaction's blanket delete forward for the one entry being deprecated. Report the removed-edge count inline to the caller, and record the removal (with tuples) in the audit log. The delete is non-fatal and a strict subset of what the tick removes; the tick compaction is unchanged and remains the backstop.

## Component Map

Single crate: `unimatrix-server`. Components below are the delivery units; pseudocode + test-plan file paths are filled during Session 2 Stage 3a.

| Component | Site | Pseudocode | Test Plan |
|-----------|------|-----------|-----------|
| eager-delete-helper | `crates/unimatrix-server/src/mcp/edge_write.rs` (new fn + `RemovedEdge` struct, beside `delete_graph_edge:244`) | pseudocode/eager-delete-helper.md | test-plan/eager-delete-helper.md |
| deprecate-handler | `crates/unimatrix-server/src/mcp/tools.rs:1413` (new step 6.5 orchestration) | pseudocode/deprecate-handler.md | test-plan/deprecate-handler.md |
| response-formatter | `crates/unimatrix-server/src/mcp/response/mutations.rs:16` (`format_status_change` / `format_deprecate_success` signature) | pseudocode/response-formatter.md | test-plan/response-formatter.md |
| audit-emit | `crates/unimatrix-server/src/server.rs:650` (`audit_fire_and_forget` call, `edge_cleanup` event) | pseudocode/audit-emit.md | test-plan/audit-emit.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

`run_orphaned_edge_compaction` (`background.rs:805`) is UNCHANGED but is a first-class dependency of the subset test (ADR-003 / AC-10) — the test invokes the real function. Include it in the integration plan even though no code changes.

## Delivery Wave Routing

All four components live in `unimatrix-server` and change in lockstep (the formatter signature change ripples to the handler). Sequence within Stage 3b:

1. **Wave A — eager-delete-helper** (`edge_write.rs`): the new `delete_agent_edges_for_entry` fn, `RemovedEdge` struct, and LOCKED predicate. No dependents until it exists.
2. **Wave A — response-formatter** (`mutations.rs`): add `edges_removed: Option<u64>` to `format_status_change` + `format_deprecate_success`; pass `None` at `format_quarantine_success` / `format_restore_success`. Independent of the helper; can land in the same wave.
3. **Wave B — deprecate-handler + audit-emit** (`tools.rs`, `server.rs`): wire step 6.5 to call the helper, thread `Some(count)`/`None` into the formatter, and fire the `edge_cleanup` audit event. Depends on both Wave A components.

Single-crate change — one PR. Waves are ordering within delivery, not separate branches.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Eager delete vs leave-to-tick | Delete eagerly at deprecation event; tick unchanged as backstop | SCOPE Goals 1/4, ADR-001 | architecture/ADR-001-eager-delete-at-deprecation-source.md |
| Edge direction | Both — inbound (`target_id=entry`) + outbound (`source_id=entry`) | SCOPE Open Questions (resolved), FR-01 | architecture/ADR-001-... |
| Provenance filter | Agent-authored only, `source = 'agent'` (single value; no distinct `human`); machine edges left to tick | SCOPE Goal 1 / Non-Goals, FR-02 | architecture/ADR-002-audit-removed-edge-tuples.md |
| Audit granularity | Tuple-level: record `(source_id, target_id, relation_type)` per removed edge via `DELETE … RETURNING`, not just a count (FIRM) | Design-review gate, ADR-002, AC-11 | architecture/ADR-002-audit-removed-edge-tuples.md |
| eager ⊆ tick enforcement | Executable test invoking both real functions over parallel fixtures; predicate LOCKED, no runtime `superseded_by` clause | SCOPE C-07, ADR-003, AC-10 | architecture/ADR-003-eager-subset-tick-invariant.md |
| Zero-case advisory | `Some(0)` renders a literal `0` in all three formats; `None` (delete failed / not run) omits the advisory | Design-review gate, ADR-004, AC-05/NFR-04/FR-03 | architecture/ADR-004-edges-removed-response-plumbing.md |
| Response plumbing | Add `edges_removed: Option<u64>` to `format_status_change` (before `format`); additive, backward-compatible | ADR-004, C-09 | architecture/ADR-004-... |
| Non-fatal handling | On any `Err`: `warn!` (not `debug`), set `edges_removed = None`, return normal success; tick backstops | SCOPE Goal 4, ADR-001, AC-06/NFR-05 | architecture/ADR-001-... |
| Helper vs inline | New `edge_write.rs` fn beside `delete_graph_edge` (not inline) | ADR-002/architecture, C-08 | architecture/ADR-002-... |

## Files to Create/Modify

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/mcp/edge_write.rs` | NEW `async fn delete_agent_edges_for_entry` + `struct RemovedEdge`; LOCKED `DELETE … RETURNING` on `write_pool_server()`; reuse `EDGE_SOURCE_AGENT` (`:28`). |
| `crates/unimatrix-server/src/mcp/tools.rs` | Insert step 6.5 in `context_deprecate` (`:1413`) after step-6 flip / past step-5 guard: call helper, thread count, fire audit. |
| `crates/unimatrix-server/src/mcp/response/mutations.rs` | Add `edges_removed: Option<u64>` param to `format_status_change` (`:16`) + `format_deprecate_success` (`:54`); render per-format; `format_quarantine_success` / `format_restore_success` pass `None`. |
| `crates/unimatrix-server/src/server.rs` | Emit `context_deprecate.edge_cleanup` `AuditEvent` via `audit_fire_and_forget` (`:650`) — only on `Ok` + non-empty. |
| Test fixtures (background-tick + mutation formatter) | EXTEND existing fixtures (cumulative) — subset test (AC-10) invokes the real `run_orphaned_edge_compaction`; per-format matrix (AC-04). |

## Data Structures

```rust
// mcp/edge_write.rs (NEW)
struct RemovedEdge {
    source_id: u64,
    target_id: u64,
    relation_type: String,
}
```

`edges_removed: Option<u64>` — `Some(n)` = eager delete ran, removed `n` (incl. `Some(0)`); `None` = delete failed / path did not delete (quarantine/restore). Count is `tuples.len()` (single source of truth, not `rows_affected()`).

`AuditEvent` (existing, `unimatrix-store/schema.rs:360`): `operation`, `target_ids: Vec<u64>`, `detail: String`, `metadata: String` (JSON).

## Function Signatures

```rust
// mcp/edge_write.rs (NEW)
async fn delete_agent_edges_for_entry(
    store: &Store,
    entry_id: u64,
) -> Result<Vec<RemovedEdge>, EdgeDeleteError>;

// LOCKED predicate — never widen by relation_type, never add runtime superseded_by clause:
// DELETE FROM graph_edges
//   WHERE (source_id = ?1 OR target_id = ?1) AND source = ?2
//   RETURNING source_id, target_id, relation_type
//   [?2 bound to EDGE_SOURCE_AGENT, write_pool_server()]

// mcp/response/mutations.rs (CHANGED)
pub fn format_status_change(
    entry: &EntryRecord,
    action: &str,
    status_key: &str,
    status_display: &str,
    reason: Option<&str>,
    edges_removed: Option<u64>,   // NEW — before `format`
    format: ResponseFormat,
) -> CallToolResult;

pub fn format_deprecate_success(
    entry: &EntryRecord,
    reason: Option<&str>,
    edges_removed: Option<u64>,   // NEW — forwarded
    format: ResponseFormat,
) -> CallToolResult;
```

Insertion (`tools.rs` `context_deprecate`, step 6.5 after step-6 flip, before step-8 format):

```
6.   let deprecated = self.deprecate_with_audit(entry_id, reason, audit_event).await?;
6.5. let removed = delete_agent_edges_for_entry(&self.store, entry_id).await;   // NON-FATAL
     //   Ok(tuples)  -> edges_removed = Some(tuples.len());
     //                  if !tuples.is_empty(): audit_fire_and_forget(edge_cleanup{ id, count, tuples })
     //   Err(e)      -> warn!(entry=id, error=e, "eager edge cleanup failed"); edges_removed = None
7.   self.services.confidence.recompute(&[deprecated.id]);   // independent fire-and-forget
8.   Ok(format_deprecate_success(&deprecated, reason, edges_removed, ctx.format));
```

### Audit record shape (`edge_cleanup`)

- `operation`: `"context_deprecate.edge_cleanup"` (distinct from the flip's `"context_deprecate"`)
- `target_ids`: `[entry_id]`
- `detail`: count summary, e.g. `"eager edge cleanup: removed {count} agent-authored edge(s) for deprecated entry #{id}"`
- `metadata`: JSON array `[{"source_id":..,"target_id":..,"relation_type":".."}, …]` — serialize via the JSON encoder (never string interpolation; must not fall through to the `"{}"` sentinel on non-empty removals).

## Constraints

- **C-01 Non-fatal** — never propagate an eager-delete error into the `context_deprecate` result. `warn!` (not `debug`, #3448), `edges_removed = None`, normal success.
- **C-02 After step-5 idempotency guard** (`tools.rs:~1442`) — re-deprecation returns early, no redundant delete.
- **C-03 After the status flip** — predicate keys on the now-non-Active entry id; order is flip → delete → format.
- **C-04 Synchronous** — `await` the delete inline; edges gone before the response is formatted (only the audit *write* is fire-and-forget).
- **C-05 Write pool** — `write_pool_server()` only.
- **C-06 Single indexed statement, both directions, agent-only** — one `DELETE … RETURNING`; filter on `source` column (F2, `background.rs:849`), never a relation-type blocklist; no per-edge loop.
- **C-07 eager ⊆ tick** — LOCKED predicate must stay a strict subset of the tick's status predicate. Enforced by AC-10 test (both real functions). If the tick predicate ever changes, re-derive the test, do not delete it.
- **C-08 New helper, not inline** — `edge_write.rs` fn beside `delete_graph_edge`, reusing its `write_pool_server()` pattern.
- **C-09 Additive response surface** — `edges_removed` optional, backward-compatible across Summary/Markdown/Json.
- **C-10 Chokepoint-only** — one production caller (`context_deprecate` step 6.5). Helper is unguarded (no status/successor check) by design; safety rests on the single call site (R-06). Any second caller is a design change, not a silent merge.
- **C-11 Compaction-as-backstop is a standing dependency invariant** — non-fatal safety depends on `run_orphaned_edge_compaction` continuing to sweep non-Active endpoints over all sources. Leave a code-adjacency comment linking the helper to the compaction (SR-05, ADR-001).

### Delivery-time closure items (from risk strategy — resolve during Stage 3b)

- **R-03 post-commit atomicity** — `DELETE … RETURNING` commits then returns rows; a marshaling `Err` after commit would delete edges with no audit record. Prove delete + tuple capture is a **single atomic statement** (one `fetch_all` on the RETURNING) so there is no "gone with no record" window. Design-closure item.
- **R-01 subset-test blind spot** — the `R ⊆ T` fixtures are successor-less, so they never exercise the one real break case. Add the explicit **chokepoint-exclusion assertion** against the real handler (successor-bearing entry never reaches the eager helper) — not prose.
- **SR-06 placement (delivery-time open item, not a blocker)** — pin `delete_agent_edges_for_entry` placement in `edge_write.rs` and confirm no ordering interaction with step-7 `confidence.recompute`. Architecture holds step 7 is independent fire-and-forget, so ordering vs step 6.5 is immaterial; place 6.5 immediately after step 6 to keep flip → delete → count → audit → format contiguous. Confirm in code.
- **count source of truth** — use `tuples.len()`, not `rows_affected()` (they agree for a single `RETURNING`, but tuples are needed for audit).

## Dependencies

Existing components reused — no new external crates, no new services:

- `graph_edges` table (`source_id`, `target_id`, `relation_type`, `source`); indexes `idx_graph_edges_source_id` (`db.rs:969`), `idx_graph_edges_target_id` (`db.rs:972`).
- `write_pool_server()`; `EDGE_SOURCE_AGENT` (`edge_write.rs:28`); `delete_graph_edge` (`edge_write.rs:244`, pattern reference).
- `deprecate_with_audit` (`server.rs:949`) → `change_status_with_audit` (`server.rs:1089`); `audit_fire_and_forget` (`server.rs:650`); `AuditEvent` (`unimatrix-store/schema.rs:360`).
- `format_deprecate_success` → `format_status_change` (`mcp/response/mutations.rs:16`).
- `run_orphaned_edge_compaction` (`background.rs:805`) — UNCHANGED backstop; invoked by the AC-10 subset test.
- `DELETE … RETURNING` support (sqlx/SQLite) — already used in `analytics.rs`.

## NOT in Scope

- Not self-learning / not drift-adaptation — deterministic graph maintenance; feeds no model, confidence, or adaptation path.
- Not detection / not a governance nudge — condition is resolved (deleted), not flagged. No `context_cycle_review` metric, no findings table, no revival of `DependencyOnDeprecatedRule` / cohesion metric (retired #891).
- No relation-type filtering — agent edges removed regardless of relation type; provenance-filtered only.
- System/machine edges (`nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8`) NOT eagerly deleted — left to the tick.
- `context_correct` / successor path excluded — already repoints inbound edges (`repoint_deprecated_target_edges`).
- No change to the `EveryTick` compaction.
- No new table, schema migration, or prune lifecycle.
- No soft-delete / undo of removed edges (irreversible by design; audit tuples are the reconstruction record).
- CoAccess cold-start migration — separate future issue.

## Alignment Status

Vision guardian: **PASS** on Vision Alignment, Milestone Fit, Scope Gaps, Risk Completeness. No blocking variance.

Two WARN items from ALIGNMENT-REPORT.md, both now settled at the design-review gate (carry as settled, not open):

1. **Zero-case advisory rendering (was a cross-document contradiction / R-04)** — RESOLVED: `Some(0)` renders a literal `0` in all three formats; `None` = delete failed / did not run → advisory omitted (ADR-004; spec AC-05 / NFR-04 / FR-03 reconciled).
2. **Audit granularity beyond count-only (Scope Additions WARN)** — CONFIRMED FIRM: tuple-level audit `(source_id, target_id, relation_type)` accepted (ADR-002; AC-11 promoted from conditional to required). Reuses the wired audit path — no new persistence; justified by SR-01 reconstructability of an irreversible delete.

Additions AC-10 (subset test) and AC-11 (tuple audit) are direct folds of scope risks SR-02 and SR-01, not net-new feature scope.
