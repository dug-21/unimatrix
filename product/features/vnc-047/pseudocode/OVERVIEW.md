# vnc-047 Pseudocode — OVERVIEW

> `context_cycle` whole-set-once run-identity `tags` → `cycle_tags` junction → surfaced in
> `context_cycle_review`. Tracks GH #940. Per-component pseudocode; read this first.
> All file:line refs verified against HEAD (branch `bugfix/893-...`) at Stage 3a.

## Components (map to ARCHITECTURE C1–C13)

| # | Component | Crate / file | Pseudocode |
|---|-----------|--------------|-----------|
| C1 | `cycle_tags` table + migration (schema v31) | `unimatrix-store/migration.rs`, `db.rs::create_tables_if_needed` | cycle_tags-migration.md |
| C2 | Store write primitive `insert_cycle_start_with_tags` (BEGIN IMMEDIATE + EXISTS guard) | `unimatrix-store/db.rs` | store-write-primitive.md |
| C3 | Store read getter `get_cycle_tags` | `unimatrix-store/db.rs` | store-read-getter.md |
| C4 | Hook tag extraction | `unimatrix-server/uds/hook.rs` | hook-extraction.md |
| C5 | Listener persistence routing | `unimatrix-server/uds/listener.rs` | listener-persistence.md |
| C6 | Tool param `CycleParams.tags` | `unimatrix-server/mcp/tools.rs` | cycle-params.md |
| C7 | Report field `RetrospectiveReport.tags` (+ SUMMARY v6) | `unimatrix-observe/types.rs`, `store/cycle_review_index.rs` | report-field.md |
| C8 | Review handler populate | `unimatrix-server/mcp/tools.rs` | review-handler.md |
| C9 | Markdown render `render_tags_section` | `unimatrix-server/mcp/response/retrospective.rs` | markdown-render.md |
| C10 | GC protection by omission + regression test | `unimatrix-store/retention.rs` | gc-protection.md |
| C11 | Deferred mutation seam (comment-only) | `unimatrix-server/mcp/tools.rs` `context_tag` | deferred-seam.md |
| C12 | Ack echo (best-effort, non-gating) | `unimatrix-server/mcp/tools.rs` context_cycle handler | ack-echo.md |
| C13 | Freeze-outcome trace (best-effort, non-gating) | inside C2 `insert_cycle_start_with_tags` | freeze-trace.md |

## Data flow (write, then read)

```
MCP context_cycle(type=start, topic=FC, goal=…, tags=[…])
  ├─ handler persists NOTHING (session-unaware). Builds best-effort ack echo.   C12
  │  tags travel via tool_input JSON, NOT via CycleParams into persistence.
  ▼
hook.rs build_cycle_event_or_fallthrough                                        C4
  │  Start only: tags = tool_input["tags"] filtered non-empty (whitespace-only dropped)
  │  if any survive → payload["tags"] = JSON Array   (parity payload["goal"])
  ▼
HookRequest::RecordEvent { ImplantEvent { payload, … } }   (transport, reuse)
  ▼
listener.rs handle_cycle_event                                                  C5
  │  Step 1  : feature_cycle = sanitize(payload["feature_cycle"])  (gate: non-empty)
  │  Step 1b : #519 pre-register evicted/absent session (attribution)  [EXISTING]
  │  Step 5  : fire-and-forget spawn, gated on !feature_cycle.is_empty():
  │             tags = payload["tags"] as array-of-strings (missing/wrong-type → [])
  │             if lifecycle == Start && !tags.is_empty()
  │                 → store.insert_cycle_start_with_tags(…)          C2  (BEGIN IMMEDIATE)
  │             else
  │                 → store.insert_cycle_event(…)  [UNCHANGED, 15 call sites]
  ▼
cycle_tags (source of truth)  +  cycle_events cycle_start row      C1  (ONE txn)   C13 trace
  │  WHOLE-SET-ONCE: EXISTS(rows for FC)? skip whole tag write : insert full submitted set

… later, at review time …

MCP context_cycle_review(topic=FC)
  ▼
tools.rs review handler (~:3409, beside goal)                                    C8
  │  report.tags = store.get_cycle_tags(FC).await.unwrap_or_default()  C3 (degrade [] + warn)
  ▼
build_cycle_review_record → serde(report) → summary_json (cycle_review_index)  [reuse]
  ├─ JSON: automatic (report.tags always serialized, even [])
  └─ Markdown: render_tags_section(report)                                       C9  ("## Tags")
```

## Shared types / structs

```rust
// C6 — mcp/tools.rs, CycleParams (additive, declares interface only; NOT the persist source)
pub tags: Option<Vec<String>>,

// C7 — unimatrix-observe/types.rs, RetrospectiveReport (additive, REQUIRED, backward-readable)
#[serde(default)]           // NO skip_serializing_if — tags ALWAYS serialize (empty → [])
pub tags: Vec<String>,

// C1 — cycle_tags junction (source of truth), entry_tags re-keyed entry_id → feature_cycle, NO FK
CREATE TABLE cycle_tags (
    feature_cycle TEXT NOT NULL,
    tag           TEXT NOT NULL,
    PRIMARY KEY (feature_cycle, tag)
);
CREATE INDEX idx_cycle_tags_tag ON cycle_tags(tag);

// C2 — new store method (UNCHANGED insert_cycle_event preserved, 15 call sites)
pub async fn insert_cycle_start_with_tags(
    &self, cycle_id: &str, seq: i64,
    phase: Option<&str>, outcome: Option<&str>, next_phase: Option<&str>,
    timestamp: i64, goal: Option<&str>, tags: &[String],
) -> Result<()>;

// C3 — new store getter (parity get_cycle_start_goal)
pub async fn get_cycle_tags(&self, feature_cycle: &str) -> Result<Vec<String>>;
```

**Verified against HEAD (Stage 3a reconciliation) — `insert_cycle_event` is UNCHANGED and its real
signature is:**

```rust
// crates/unimatrix-store/src/db.rs:320 (single definition, grep-confirmed)
pub async fn insert_cycle_event(
    &self, cycle_id: &str, seq: i64, event_type: &str,
    phase: Option<&str>, outcome: Option<&str>, next_phase: Option<&str>,  // next_phase IS present
    timestamp: i64, goal: Option<&str>,                                    // NO goal_embedding arg
) -> Result<()>;
```

A Stage 3a tester note claiming the signature omits `next_phase` and carries `goal_embedding` is
**incorrect against HEAD**. `insert_cycle_start_with_tags` (C2) therefore mirrors these 8 columns
exactly. `goal_embedding` is a nullable BLOB left NULL by the INSERT on BOTH paths and populated
afterward by the existing fire-and-forget `update_cycle_start_goal_embedding` UPDATE (db.rs:438) in
listener Step 6 (:3081) — do NOT add it to the C2 INSERT. See store-write-primitive.md (C2) and
listener-persistence.md (C5).

`payload["tags"]` contract (C4 → C5): a JSON array of non-empty strings, OR the key is
absent. A wrong type (object/scalar) or absent key MUST degrade to an empty tag list —
never panic. `payload["tags"]` is the ONLY persistence carrier (SR-03).

## Two independent version cascades — DO NOT LUMP (SR-01, gate miss #4153/#4373)

| Cascade | Constant | Kind | Where | Component |
|---------|----------|------|-------|-----------|
| #1 schema v31 | `CURRENT_SCHEMA_VERSION` 30→31 (`migration.rs:26`) | **real DB migration**, 3 paths (fresh-create, migration step, idempotency guard) + pinned test | migration.rs, db.rs | C1 |
| #2 summary v6 | `SUMMARY_SCHEMA_VERSION` 5→6 (`cycle_review_index.rs:54`) | **fidelity stamp** (NO DB migration): const bump + all `RetrospectiveReport` construction sites (compiler-enforced) + `#[serde(default)]` backward-read + pinned test (:709-716) | cycle_review_index.rs, types.rs | C7 |

Each is proven by discrete per-path assertions + its own pinned test. SUMMARY v6 additionally
requires a `#[serde(default)]` backward-read test of a v5 blob (no `tags` key → empty vec).
**Re-verify at implementation start** that 31 and 6 are still the next free numbers at HEAD (SR-02/R-10);
if either is taken by a parallel merge, flag for renumber before proceeding.

## Sequencing constraints (build order for Stage 3b)

1. **C1 first** (table + both version cascades' schema path) — nothing persists or reads without the table.
2. **C7** (report field + SUMMARY v6) — independent of C1's DB migration; can proceed in parallel, but C8/C9 depend on it.
3. **C2, C3** (store primitives) depend on C1.
4. **C4 → C5** (hook → listener) depend on C2. C5 is the single routing decision point (keeps 15 `insert_cycle_event` sites untouched).
5. **C6** (param) is independent (interface declaration only).
6. **C8, C9** (review populate + render) depend on C3 and C7.
7. **C10** (GC omission + regression test) depends on C1 (table exists to protect).
8. **C11, C12, C13** are best-effort/comment-only — independent, NON-GATING.

## Cross-cutting invariants (every component honors these)

- **Value-opacity.** Non-empty is the ONLY check. No vocabulary/allow-list/length/charset/prefix
  validation, no namespace derivation, no `MAX_*` byte cap (unlike `goal`). Tags stored/returned verbatim.
- **Whole-set-once, not per-row.** First tag-bearing start freezes the entire set; the freeze
  decision reads row *existence* only, never tag values. Per-key/per-namespace write-once is REJECTED.
- **Hook-path only.** Exactly one persistence route (C2, reached only from C5). Bare MCP handler persists nothing.
- **Parameterized binds only.** Opacity forbids validation, so parameterization is the ONLY SQLi
  defense (load-bearing). No `LIKE`/`like_escape` on the cycle-tag write path.
- **Fire-and-forget.** Write errors → `tracing::warn`, no caller signal. Read errors → `[]` + warn, review never fails.
- **Best-effort layers (C11/C12/C13) never block a gate.** Frozen-skip is NOT caller-returnable.
