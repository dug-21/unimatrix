# vnc-047 Implementation Brief — `context_cycle` whole-set-once run-identity tags

> Compiled from Session 1 design artifacts (regenerated after 2026-07-09 source revision). Tracks GH #940.
> Tag-model precedent: vnc-045 (#928). Cycle-attribute precedent: col-025 (#3396).
> Schema versions re-verified free at HEAD at synthesis (2026-07-09):
> `CURRENT_SCHEMA_VERSION = 30` (v31 free), `SUMMARY_SCHEMA_VERSION = 5` (v6 free).
> **SR-02/NFR-3/R-10: re-verify BOTH again at implementation start — a parallel merge may claim either.**

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-047/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-047/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-047/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-047/architecture/ARCHITECTURE.md |
| ADR-001 (cycle_tags junction, schema v31) | product/features/vnc-047/architecture/ADR-001-cycle-tags-junction-and-schema-v31.md |
| ADR-002 (whole-set-once write on hook cycle_start txn) | product/features/vnc-047/architecture/ADR-002-set-once-write-on-hook-cycle-start-transaction.md |
| ADR-003 (fire-and-forget durability envelope) | product/features/vnc-047/architecture/ADR-003-fire-and-forget-durability-envelope-absent-session.md |
| ADR-004 (RetrospectiveReport.tags, SUMMARY v6) | product/features/vnc-047/architecture/ADR-004-retrospectivereport-tags-surface-and-summary-v6.md |
| ADR-005 (GC protection by omission) | product/features/vnc-047/architecture/ADR-005-gc-protection-by-omission.md |
| ADR-006 (deferred mutation home on context_tag) | product/features/vnc-047/architecture/ADR-006-deferred-mutation-home-on-context-tag.md |
| ADR-007 (best-effort ack echo, non-gating) | product/features/vnc-047/architecture/ADR-007-ack-echo-best-effort.md |
| Risk-Based Test Strategy | product/features/vnc-047/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-047/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-047/ACCEPTANCE-MAP.md |

## Goal

Let a `context_cycle` run carry opaque, engine-uninterpreted **`tags`** — a general run-identity
label array (workflow version, run mode, confidence-required, experiment arm, etc.; not
workflow-only) — frozen as a **whole set** at the first tag-bearing cycle start, stored durably as
the source of truth in a new `cycle_tags` junction table, and surfaced per-run in
`context_cycle_review` (markdown + JSON). It reuses the existing entry-tag opacity model ("one tag
model") and adds no new MCP tool — only an additive `tags` param on `context_cycle`. The tags become
an externally joinable label substrate for later A/B analysis; the cross-run join is out of scope.

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| C1 `cycle_tags` table + migration (schema v31) | pseudocode/cycle_tags-migration.md | test-plan/cycle_tags-migration.md |
| C2 store write primitive `insert_cycle_start_with_tags` (BEGIN IMMEDIATE + EXISTS guard) | pseudocode/store-write-primitive.md | test-plan/store-write-primitive.md |
| C3 store read getter `get_cycle_tags` | pseudocode/store-read-getter.md | test-plan/store-read-getter.md |
| C4 hook tag extraction (`build_cycle_event_or_fallthrough`) | pseudocode/hook-extraction.md | test-plan/hook-extraction.md |
| C5 listener persistence (`handle_cycle_event` step-5) | pseudocode/listener-persistence.md | test-plan/listener-persistence.md |
| C6 tool param (`CycleParams.tags`) | pseudocode/cycle-params.md | test-plan/cycle-params.md |
| C7 report field (`RetrospectiveReport.tags`) | pseudocode/report-field.md | test-plan/report-field.md |
| C8 review handler populate | pseudocode/review-handler.md | test-plan/review-handler.md |
| C9 markdown render (`render_tags_section`) | pseudocode/markdown-render.md | test-plan/markdown-render.md |
| C10 GC protection by omission + regression test | pseudocode/gc-protection.md | test-plan/gc-protection.md |
| C11 deferred mutation seam (comment-only) | pseudocode/deferred-seam.md | test-plan/deferred-seam.md |
| C12 ack echo (best-effort, non-gating) | pseudocode/ack-echo.md | test-plan/ack-echo.md |
| C13 freeze-outcome trace (listener, non-gating) | pseudocode/freeze-trace.md | test-plan/freeze-trace.md |

Note: Stage 3a COMPLETE (2026-07-09). All 14 pseudocode and 14 test-plan files exist at the paths
above (C1–C13 + OVERVIEW), verified against HEAD. C2 signature reconciled: `insert_cycle_start_with_tags`
mirrors HEAD `insert_cycle_event` (db.rs:320) — carries `next_phase`, does NOT write `goal_embedding`
(populated later by `update_cycle_start_goal_embedding` on `event_type='cycle_start'`). C8 review-populate
extracted as `pub(crate) populate_review_tags` seam for the AC-05 assembled-path test.

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Schema placement (OQ-1) | New `cycle_tags(feature_cycle, tag)` junction, PK `(feature_cycle, tag)`, index on `(tag)`, no FK; source of truth. NOT `cycle_review_index` (does not exist during run), NOT `sessions` (purgeable), NOT `cycle_events` (per-event row, not queryable by tag). | SCOPE OQ-1; ARCH C1 | ADR-001-cycle-tags-junction-and-schema-v31.md |
| Schema cascade #1 | `CURRENT_SCHEMA_VERSION` 30→31 — real DB migration on THREE paths: fresh-create (`create_tables_if_needed`), migration step (`if current_version < 31`), idempotency guard (`CREATE TABLE/INDEX IF NOT EXISTS`) + pinned test. | SCOPE AC-03; SR-01 | ADR-001 |
| Set-once semantics (OQ-2) | **WHOLE-SET-ONCE.** The first **tag-bearing** cycle start freezes the ENTIRE set for the `feature_cycle`; every later start (same / subset / superset / different) is a WHOLE-SET no-op — never merged, never accumulated. A **tagless** start does NOT lock (first *tags* win, not first start). Per-key / per-namespace write-once was explicitly REJECTED — it would force namespace parsing and break value-opacity. | SCOPE OQ-2, Non-Goal #2; ARCH ADR-002 | ADR-002 |
| Freeze mechanism + race safety | Enforced by a row-existence guard `SELECT EXISTS(SELECT 1 FROM cycle_tags WHERE feature_cycle=?)` inside the cycle_start transaction; if none exist, insert the full submitted set, else skip the whole tag write. The txn is opened with **`BEGIN IMMEDIATE`** (not sqlx's default DEFERRED `pool.begin()`) so the guard takes the write lock up front and is TOCTOU-safe against a concurrent same-cycle start (R-15). Guard reads existence only — never parses tag values or namespaces (value-opacity). | ARCH ADR-002 §3; R-15 | ADR-002 |
| Write route (OQ-3, SR-03) | Hook path ONLY: `build_cycle_event_or_fallthrough` → `RecordEvent` → `handle_cycle_event` → `insert_cycle_start_with_tags` in the same txn as the cycle_start insert. Bare MCP handler persists nothing. NO second persistence route. Value read from `tool_input["tags"]`, not from `CycleParams`. `insert_cycle_event` signature UNCHANGED (15 call sites preserved). | SCOPE OQ-3; ARCH Constraint | ADR-002 |
| Absent/evicted-session durability (SR-07) | Persistence gate = `!feature_cycle.is_empty()` (same step-5 gate as goal); does NOT require registry presence. #519 pre-register (step-1b) restores attribution. Empty/missing `feature_cycle` = the single documented silent drop. DB error → `tracing::warn`, no caller signal (set-and-forget). | ARCH Error boundaries; SR-07 | ADR-003 |
| Read/surface path (OQ-4) | Post-review only via `context_cycle_review`; no pre-review read surface. New `get_cycle_tags(feature_cycle)` getter (`SELECT tag … ORDER BY tag`, parity `get_cycle_start_goal`); populate `report.tags`; rides `summary_json`; JSON automatic, markdown via `render_tags_section`. Read degrades to `[]` + warn on error. | SCOPE OQ-4; ARCH C3/C8/C9 | ADR-004 |
| Schema cascade #2 | `SUMMARY_SCHEMA_VERSION` 5→6 — fidelity STAMP (no DB migration): bump const, update pinned test (#4178/#5051), all `RetrospectiveReport` construction sites (compiler-enforced by required field). `#[serde(default)]` for backward-read of v5 blobs. | SCOPE AC-05; SR-01; crt-055 | ADR-004 |
| GC durability (SR-09) | Protection by **OMISSION**, not a protected-set registration — there is no protected-set data structure in `retention.rs`. Do NOT add `cycle_tags` to any DELETE path in `gc_cycle_activity` (:116) or `gc_unattributed_activity` (:202); extend `test_gc_protected_tables_regression` across BOTH surfaces with a positive control. | SCOPE AC-04; ARCH C10 | ADR-005 |
| Deferred mutation home (OQ-3) | Reserved as a comment-only seam on the existing `context_tag` tool (additive, entry-defaulting `target`); NOT a new `context_cycle_tag` tool, NOT built now. `context_correct` and entry `context_tag` untouched. | SCOPE Non-Goal #1; ARCH C11 | ADR-006 |
| Ack echo + freeze trace (best-effort, NON-GATING) | On the EXISTING `context_cycle` ack string: start-with-tags → "N labels accepted for recording… use context_cycle_review to confirm" (accept-for-recording, NOT a durability guarantee); non-start-with-tags → "tags ignored — only recorded at cycle start". Listener emits a `tracing` line distinguishing wrote-set vs frozen-skip. NO new interface, NO read-back API; frozen-skip is NOT caller-returnable. `cycle_review` is authoritative. **Never blocks a gate.** | SCOPE Goal #4 note, AC-08; SPEC FR-12/AC-09; R-16 | ADR-007 |
| No back-fill (SR-10) | Staleness advisory-only; pre-v6 cached reviews never recompute. Historical cycles show empty `## Tags` by design. | SCOPE; ARCH ADR-004 §6 | ADR-004 |

> ADR provenance in Unimatrix (updated 2026-07-09): ADR-002 = entry **#5658** (was #5652, updated via
> context_correct); ADR-007 = entry **#5659**. Entry-tag primitive port anchor: #5599.

## Files to Create / Modify

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/migration.rs` | BUMP `CURRENT_SCHEMA_VERSION` 30→31 (:26); add `if current_version < 31` migration step (after the `< 30` block, ~:1474) creating `cycle_tags` + `idx_cycle_tags_tag` with `CREATE TABLE/INDEX IF NOT EXISTS`. |
| `crates/unimatrix-store/src/db.rs` | Add `CREATE TABLE IF NOT EXISTS cycle_tags` + index to `create_tables_if_needed` (fresh-create, ~:534, beside `entry_tags`); add `insert_cycle_start_with_tags(...)` — **BEGIN IMMEDIATE** txn: cycle_start INSERT + whole-set-once EXISTS guard (insert full set or skip); add `get_cycle_tags(feature_cycle)`. Leave `insert_cycle_event` UNCHANGED. |
| `crates/unimatrix-server/src/uds/hook.rs` | In `build_cycle_event_or_fallthrough` (:769), beside goal extraction, extract `tags` Start-only, non-empty-filtered, into `payload["tags"]` (JSON array). Infallible (filter, no error). |
| `crates/unimatrix-server/src/uds/listener.rs` | In `handle_cycle_event` step-5 spawn (~:3062), read `payload["tags"]`; route `Start && !tags.is_empty()` → `insert_cycle_start_with_tags`, else → `insert_cycle_event` (unchanged). Gate on `!feature_cycle.is_empty()`, NOT on attribution_result. Emit best-effort `tracing` line: wrote-set vs frozen-skip (C13, non-gating). |
| `crates/unimatrix-server/src/mcp/tools.rs` | Add `tags: Option<Vec<String>>` to `CycleParams` (~:515-542). In cycle_review handler (~:3409-3428, beside goal) populate `report.tags = store.get_cycle_tags(&fc).await.unwrap_or_default()` (degrade + warn). Add best-effort ack echo in the `context_cycle` handler `response_text` (~:4154-4160, goal-ack precedent) — start-with-tags accepted-for-recording, non-start ignored (C12, non-gating). Add comment-only deferred-mutation seam near `context_tag` handler (~:1558-1614). No new tool. |
| `crates/unimatrix-observe/src/types.rs` | Add `#[serde(default)] pub tags: Vec<String>` to `RetrospectiveReport` (after `goal`, ~:436). Required field (not Option) → compiler enforces all construction sites. |
| `crates/unimatrix-server/src/mcp/response/retrospective.rs` | Add `render_tags_section(&RetrospectiveReport) -> String` (`## Tags`, parity `render_goal_section`); call immediately after `render_goal_section` (~:49). Empty tags → explicit "No tags." line. |
| `crates/unimatrix-store/src/cycle_review_index.rs` | BUMP `SUMMARY_SCHEMA_VERSION` 5→6 (:54); update pinned assertion + message (~:709-716) to reference vnc-047. |
| `crates/unimatrix-store/src/retention.rs` | NO code change to DELETE paths (protection by omission). Extend `test_gc_protected_tables_regression` (~:521) to seed/snapshot `cycle_tags` across both GC surfaces with a positive control. |

## Data Structures

```sql
CREATE TABLE cycle_tags (
    feature_cycle TEXT NOT NULL,
    tag           TEXT NOT NULL,
    PRIMARY KEY (feature_cycle, tag)
);
CREATE INDEX idx_cycle_tags_tag ON cycle_tags(tag);
-- entry_tags re-keyed entry_id → feature_cycle. No FK (feature_cycle is free-text, no parent table).
-- PK retained for row integrity; the whole-set FREEZE is the EXISTS guard + BEGIN IMMEDIATE, NOT the PK/ON CONFLICT.
```

```rust
// CycleParams (mcp/tools.rs) — additive
tags: Option<Vec<String>>,

// RetrospectiveReport (unimatrix-observe/types.rs) — additive, required, backward-readable
#[serde(default)]
pub tags: Vec<String>,
```

## Function Signatures

```rust
// unimatrix-store/src/db.rs (NEW)
pub async fn insert_cycle_start_with_tags(
    &self, cycle_id: &str, seq: i64,
    phase: Option<&str>, outcome: Option<&str>, next_phase: Option<&str>,
    timestamp: i64, goal: Option<&str>, tags: &[String],
) -> Result<()>;
// BEGIN IMMEDIATE txn:
//   (a) cycle_start INSERT identical to insert_cycle_event (event_type = "cycle_start");
//   (b) WHOLE-SET-ONCE guard: if NOT EXISTS(SELECT 1 FROM cycle_tags WHERE feature_cycle=?1)
//         { INSERT every submitted tag: cycle_tags(feature_cycle, tag) }  // parameterized binds
//       else { skip the entire tag write — set is frozen }
//   Duplicate tag within one submitted set → ON CONFLICT(feature_cycle, tag) DO NOTHING (no txn abort).

// unimatrix-store/src/db.rs (NEW), parity get_cycle_start_goal (:371)
pub async fn get_cycle_tags(&self, feature_cycle: &str) -> Result<Vec<String>>;
// SELECT tag FROM cycle_tags WHERE feature_cycle = ?1 ORDER BY tag   (deterministic)

// unimatrix-server/src/mcp/response/retrospective.rs (NEW)
fn render_tags_section(report: &RetrospectiveReport) -> String;   // "## Tags"

// UNCHANGED (15 call sites preserved):
// Store::insert_cycle_event(cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal)
```

## Constraints

- **Hook-path only.** Bare MCP `context_cycle` handler persists nothing (tools.rs:4062). Tags MUST
  ride `hook.rs` → `handle_cycle_event`, written in the `cycle_start` transaction. No second
  persistence route (SR-03/R-06/ADR-002).
- **Whole-set-once freeze + atomic race safety.** First tag-bearing start freezes the entire set;
  later starts (any set) are whole-set no-ops; tagless start does not lock. Enforced by an EXISTS
  guard inside a **`BEGIN IMMEDIATE`** transaction that also carries the cycle_start event row and
  the per-tag inserts — guard + start row + tag rows are ONE atomic unit (R-05/R-15/ADR-002).
  Per-key/per-namespace write-once is REJECTED (would break value-opacity).
- **Value-opacity.** Non-empty is the ONLY check — no vocabulary, allow-list, length, charset, or
  prefix validation; no `protected_tags` policy; no engine-side namespace derivation. The freeze
  guard reads row existence only, never tag values. Tags stored/returned verbatim. NO length cap
  (unlike `goal`'s `MAX_GOAL_BYTES`; DoS accepted under Write gate) (vnc-045 SD-8).
- **SQL injection defense.** Parameterized binds only (parity `add_tag` write.rs:281) — because
  opacity forbids validation, parameterization is the ONLY SQLi defense (load-bearing). No
  `LIKE`/`like_escape` on the cycle-tag write path (no namespace query ships).
- **Two independent version cascades.** v31 (DB migration, ADR-001) and SUMMARY v6 (fidelity stamp,
  ADR-004) are separate; each needs its full per-path update + pinned test. Recurring gate miss
  (#4153, #4373) — do NOT lump them (SR-01/R-01/R-02).
- **Authorization.** Single `Capability::Write` gate (parity `context_tag` tools.rs:1562). No
  `Capability::Tag`. `agent_id` is audit-only; does NOT authorize or scope the write. **No per-tag
  audit event** is emitted (see Alignment Status note 2).
- **GC protection by omission** — not a protected-set registration (ADR-005). See Alignment Status.
- **Ack echo / freeze trace are best-effort, NON-GATING** (ADR-007) — must not block any gate; no
  new interface; frozen-skip not caller-returnable.
- Rust workspace rules: file-size limits; extend existing fixtures/helpers (test infra is
  cumulative); Grep/Glob not Bash for search.

## Dependencies

- **Crates (in-workspace):** `unimatrix-store`, `unimatrix-server`, `unimatrix-observe`. No new
  external crates.
- **Precedent features (read, do not modify):**
  - vnc-045 (#928) `context_tag` / `entry_tags` — tag opacity (SD-8), the junction storage +
    parameterized `INSERT` primitive (write.rs:281, entry #5599; ported and re-keyed
    `entry_id → feature_cycle`). NB the *freeze* mechanism diverges deliberately: whole-set EXISTS
    guard, NOT per-row `ON CONFLICT` accumulate. Write-gate + agent_id-audit-only posture (SD-9)
    matches; vnc-045's `remove`/`replace` verbs and its dedicated `operation="context_tag"` audit
    event are NOT used.
  - col-025 (#3396) `goal` on cycle_events — end-to-end hook→persist→surface path cloned
    (entries #3396 storage-on-start-row, #3399 degrade-to-None read contract). The `goal`
    fire-and-forget ack (tools.rs:4154-4160) is the exact precedent for the ADR-007 echo.
  - crt-036 `retention.rs` — the GC surface `cycle_tags` must survive.
  - crt-055 / #4178, #5051 — `SUMMARY_SCHEMA_VERSION` bump discipline + pinned test.
  - nxs-008 / #360 / #373 — `entry_tags` junction model + junction-vs-JSON rule (pattern #373).

## NOT in Scope

1. Tag mutation on a cycle (no add/remove/replace after start). Deferred mutation home reserved on
   existing `context_tag` (additive, entry-defaulting) — NOT a new `context_cycle_tag` tool, NOT built.
2. **Per-key / per-namespace write-once** — REJECTED; freeze is whole-set only (a single row-existence
   check), never per-tag/per-namespace (which would require namespace parsing).
3. Any new MCP op or `context_cycle` interface change beyond the additive `tags` param. (The ADR-007
   ack echo reuses the EXISTING ack string — no new interface.)
4. Cross-cycle QUERY/SEARCH by tag — no cycle-list/search surface exists; deferred companion issue.
   `cycle_tags`'s `(tag)` index is the substrate for it (shaped so v2 needs no re-migration).
5. Cross-run comparison / diffing / A/B aggregation — external, out-of-band. Mid-run/outcome
   (`cycle_stop`) labels, post-start mutation, and cross-cycle read are all part of the SAME deferred
   future discussion, not separate tracks.
6. Vocabulary, allow-list, length/charset validation, prefix enforcement, `protected_tags` policy.
7. A pre-review cycle-tag read surface. Read-back via `context_cycle_review` only; the frozen-skip
   outcome is NOT caller-returnable (trace only).
8. Modifying entry `context_tag` / `context_correct`.
9. Trust-level / identity authorization (`Capability::Write` only; `agent_id` audit-only).
10. Back-fill of historical / pre-v6 cached reviews (no recompute; empty `## Tags` by design).

## Alignment Status

From ALIGNMENT-REPORT.md (reviewed 2026-07-08, re-verified 2026-07-09 post-revision): **no VARIANCE
or FAIL. Verdict PASS.** Vision Alignment, Scope Gaps, Architecture Consistency, Risk Completeness
all PASS. The feature stays in the observe/annotate lane; whole-set-once reinforces value-opacity
(the per-namespace rule was rejected specifically to avoid namespace parsing).

**TWO accept-recommended WARNs (both human-acknowledgment, not rework):**

- **WARN-1 (deferred external payoff — already accepted):** Milestone/Goal Fit — the feature ships a
  substrate whose consumer (cross-run A/B) is external and deferred; it advances no strategic goal's
  claim-floor today. Nearest ties: self-learning intelligence (#5518, the `(tag)` index shaped for
  the deferred learn-by-tag direction) and, post general-run-identity reframing, domain-agnostic
  (#5517, non-SDLC labels). Human-accepted (SR-04). Acknowledge at demo that no in-product A/B
  consumer ships.
- **WARN-2 (ack echo is a scope addition — new):** The best-effort ack echo (ADR-007/FR-12/AC-09) is
  additive beyond SCOPE.md's goals 1-6. Mitigating: no new MCP interface (reuses the goal ack
  precedent), explicitly best-effort / non-gating, respects Non-Goal #6 (echoes the caller's own
  input as "accepted for recording," never reads stored `cycle_tags`; frozen-skip stays trace-only),
  honestly worded as accept-for-recording not a durability guarantee. **Recommendation: Accept with
  acknowledgment** — proportionate and interface-stable; no rework warranted. Alternative if strict
  scope hygiene is preferred: defer FR-12/AC-09 to a follow-up.

**Two implementer accuracy notes (carry into delivery):**
1. **GC protection is by OMISSION** from the retention DELETE paths, not a protected-set
   registration — there is no protected-set data structure in `retention.rs`. Do not "register";
   simply do not reference `cycle_tags` in any DELETE, and prove omission via the extended
   regression test across BOTH `gc_cycle_activity` and `gc_unattributed_activity`. Follow ADR-005.
2. **No dedicated `context_tag` audit event** — unlike vnc-045's entry path (`operation="context_tag"`),
   vnc-047 emits none. Agent attribution rides the `cycle_start` event, not a per-tag audit record.
   Authz posture matches vnc-045 SD-9 (Write-only, agent_id non-authorizing) but the audit-event
   emission does NOT. Do not overstate "SD-9 parity."

## Gate-Critical Coverage Obligations (from RISK-TEST-STRATEGY §Coverage Summary)

1. **Assembled-path tests for AC-02 and AC-05** — `proven_by` MUST cite a test driving
   MCP→hook→listener→`cycle_review`, not a store-only structural test (R-03/SR-08).
2. **Two independent version cascades**, each proven by discrete per-path assertions + its pinned
   test; SUMMARY v6 requires a `#[serde(default)]` backward-read test of a v5 blob (R-01/R-02/SR-01).
3. **`test_gc_protected_tables_regression` extended across BOTH GC DELETE surfaces** with a positive
   control (something IS purged, e.g. `sessions`) so it can't pass vacuously (R-07/SR-09).
4. **Absent/evicted-session persistence exercised on the assembled path** (#519 pre-register,
   step-1b) — not asserted by inspection (R-04/SR-07).
5. **Whole-set-once proven by EXACT stored-set equality** across changed/subset/superset/different
   re-starts + the tagless-start-does-not-lock case, PLUS `BEGIN IMMEDIATE` verified and a concurrent
   same-cycle-start test asserting exactly one intact whole set (no merge) (R-08/R-15/SR-05).
6. **SR-02 re-verification** of `CURRENT_SCHEMA_VERSION == 30` and `SUMMARY_SCHEMA_VERSION == 5` at
   HEAD at implementation start, recorded in the coverage report; flag renumber if either is taken.

**Explicitly NON-gating:** the best-effort ack echo (C12) + listener freeze trace (C13) —
FR-12/AC-09/R-16. Verify the echo strings and tracing line if implemented; do NOT block delivery on
them. The frozen-skip outcome is NOT caller-returnable, so no assembled-path proof is required for it.
