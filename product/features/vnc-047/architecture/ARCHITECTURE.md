# vnc-047 Architecture — `context_cycle` tags (whole-set-once run-identity labels)

> Tracks GH #940. Companion ADRs: ADR-001..ADR-007 in this directory.
> All file:line references verified against HEAD (branch `bugfix/893-...`) during design.

## System Overview

`context_cycle` runs today carry exactly one engine-uninterpreted attribute — `goal` (col-025) —
written on the UDS hook path and surfaced in `context_cycle_review`. vnc-047 adds a second such
attribute, **`tags`**: an opaque array of **run-identity labels** (workflow version, run mode,
confidence-required, arm, etc. — not workflow-only), set once at cycle start as a **frozen whole
set**, stored in a new durable junction table, and surfaced in review. It is a near-clone of the
`goal` end-to-end path (col-025) crossed with the entry-tag storage/opacity model (vnc-045 / nxs-008
`entry_tags`) — the "one tag model" north star of the issue. The engine never parses label values or
namespaces (value-opacity, vnc-045 SD-8).

Three subsystems are touched, in the same order data flows:

1. **Storage substrate** (`unimatrix-store`) — a new `cycle_tags(feature_cycle, tag)` junction is the
   single source of truth. Requires `CURRENT_SCHEMA_VERSION` 30→31 (ADR-001) and GC protection
   (ADR-005).
2. **Write path** (`unimatrix-server/uds`) — tags ride the existing cycle_start hook path
   (`build_cycle_event_or_fallthrough` → `RecordEvent` → `handle_cycle_event`) and are persisted in
   the **same transaction** as the `cycle_start` event insert with a **whole-set-once EXISTS guard**
   (the first tag-bearing start freezes the entire set; ADR-002), inheriting the fire-and-forget
   durability envelope (ADR-003). A best-effort ack echo reports acceptance-for-recording (ADR-007).
3. **Read/surface path** (`unimatrix-server/mcp`, `unimatrix-observe`) — a new `get_cycle_tags` getter
   feeds a new `RetrospectiveReport.tags` field that rides `summary_json` into `context_cycle_review`
   markdown + JSON. Requires `SUMMARY_SCHEMA_VERSION` 5→6 (ADR-004).

The two version bumps are **independent cascades** (SR-01) and are deliberately split across ADR-001
(schema v31, a real DB migration) and ADR-004 (summary v6, a round-trip fidelity stamp — no DB
migration). Each must be re-verified free at implementation start (SR-02).

## Component Breakdown

| # | Component | Crate / file | Responsibility (this feature) |
|---|-----------|--------------|-------------------------------|
| C1 | `cycle_tags` table + migration | `unimatrix-store/src/migration.rs`, `db.rs::create_tables_if_needed` | Durable source-of-truth junction; fresh-create + v30→v31 migration. ADR-001 |
| C2 | Store write primitive | `unimatrix-store/src/db.rs` | New `insert_cycle_start_with_tags` (BEGIN IMMEDIATE txn: cycle_start event + whole-set-once EXISTS guard → insert full set or skip). ADR-002 |
| C3 | Store read getter | `unimatrix-store/src/db.rs` | New `get_cycle_tags(feature_cycle)` (parity `get_cycle_start_goal`). ADR-004 |
| C4 | Hook extraction | `unimatrix-server/src/uds/hook.rs` | `build_cycle_event_or_fallthrough` extracts `tags` (Start only, non-empty filter) into `payload["tags"]`. ADR-002 |
| C5 | Listener persistence | `unimatrix-server/src/uds/listener.rs` | `handle_cycle_event` reads `payload["tags"]`, routes Start-with-tags to C2. ADR-002/003 |
| C6 | Tool param | `unimatrix-server/src/mcp/tools.rs` `CycleParams` | Additive `tags: Option<Vec<String>>` (declares the interface; AC-06). ADR-002 |
| C7 | Report field | `unimatrix-observe/src/types.rs` `RetrospectiveReport` | New required `tags: Vec<String>` field. ADR-004 |
| C8 | Review handler | `unimatrix-server/src/mcp/tools.rs` (cycle_review, ~:3410) | Read C3, populate `report.tags`; rides `summary_json`. ADR-004 |
| C9 | Markdown render | `unimatrix-server/src/mcp/response/retrospective.rs` | New `render_tags_section` after `render_goal_section`. ADR-004 |
| C10 | GC protection | `unimatrix-store/src/retention.rs` | `cycle_tags` omitted from all DELETE paths; regression test extended. ADR-005 |
| C11 | Deferred mutation seam | `unimatrix-server/src/mcp/tools.rs` `context_tag` | Reserved (comment-only), not built. ADR-006 |
| C12 | Ack echo (response) | `unimatrix-server/src/mcp/tools.rs` context_cycle handler (~:4154) | Best-effort tag phrase in the existing ack: Start→accepted-for-recording; non-start→ignored. ADR-007 |
| C13 | Freeze-outcome trace | `unimatrix-server/src/uds/listener.rs` (step-5 spawn) | `tracing` wrote-set vs frozen-skip; NOT returned to caller. ADR-007 |

## Component Interactions / Data Flow

```
MCP context_cycle(type=start, topic=FC, tags=[...])   [handler persists NOTHING; builds ack echo C12]
        │  (tags travel via tool_input JSON, NOT via the MCP handler — see Constraint below)
        │  handler ack (C12, ADR-007): Start w/tags → "N labels accepted for recording…";
        │                              non-start w/tags → "tags ignored — start only"
        ▼
hook.rs build_cycle_event_or_fallthrough                       C4
        │  Start only: tags_opt = tool_input["tags"] filtered non-empty
        │  payload["tags"] = JSON array   (parity with payload["goal"], hook.rs:877-880)
        ▼
HookRequest::RecordEvent { ImplantEvent { payload, ... } }     (transport)
        ▼
listener.rs handle_cycle_event                                 C5
        │  Step 1: feature_cycle from payload (gate: non-empty)
        │  Step 1b: #519 pre-register evicted session (attribution only)
        │  Step 5 spawn (fire-and-forget):
        │      if Start && tags non-empty → C2 insert_cycle_start_with_tags(...)   ← BEGIN IMMEDIATE txn
        │      else                        → insert_cycle_event(...)  [UNCHANGED]
        │      trace (C13): wrote-set | frozen-skip
        ▼
cycle_tags (source of truth)  +  cycle_events.cycle_start row   C1  (ONE transaction)
        │  WHOLE-SET-ONCE: NOT EXISTS(rows for FC) ? insert full set : skip entire tag write

... later, at review time ...

MCP context_cycle_review(topic=FC)
        ▼
tools.rs review handler                                         C8
        │  tags = store.get_cycle_tags(FC)      C3   (degrade to [] on error)
        │  report.tags = tags
        ▼
build_cycle_review_record → serde(report) → summary_json (cycle_review_index)
        ├─ JSON output: automatic (report.tags serialized)
        └─ Markdown: render_tags_section(report)   C9  ("## Tags")
```

### Error boundaries
- **Write (C2/C5):** fire-and-forget `tokio::spawn`; a DB error only emits `tracing::warn` (parity
  with `insert_cycle_event` failure at listener.rs:3077). No caller-visible success/failure signal —
  set-and-forget (SR-03). Persistence is gated solely on `!feature_cycle.is_empty()` (ADR-003).
- **Read (C8):** `get_cycle_tags` error degrades `report.tags` to empty vec + `tracing::warn` (parity
  with the `get_cycle_start_goal` degrade arm at tools.rs:3425). Review never fails on tag read.

## Technology Decisions (see ADRs for rationale)

| ADR | Decision |
|-----|----------|
| ADR-001 | `cycle_tags` junction is the source of truth; `CURRENT_SCHEMA_VERSION` 30→31 (cascade #1) |
| ADR-002 | **Whole-set-once** write on the hook cycle_start transaction (BEGIN IMMEDIATE + EXISTS guard freezes the entire set); ports entry-tag storage; no second persistence route |
| ADR-003 | Fire-and-forget durability envelope; tags persist iff payload `feature_cycle` non-empty; absent-session parity via #519 pre-register |
| ADR-004 | `RetrospectiveReport.tags` rides `summary_json`; `SUMMARY_SCHEMA_VERSION` 5→6 (cascade #2); no back-fill of pre-v6 reviews |
| ADR-005 | GC protection is by **omission** from retention DELETE paths (correction to the SCOPE "protected set" phrasing) + regression-test extension |
| ADR-006 | Future tag-mutation home reserved on the existing `context_tag` tool (seam only, not built) |
| ADR-007 | Best-effort ack echo in the existing context_cycle response (accept-for-recording); frozen-skip surfaced as listener tracing only; no new interface |

## Integration Points (dependencies)

- **vnc-045 / nxs-008** (`entry_tags`, `context_tag`) — the tag junction storage + value-opacity model
  ported (re-keyed `entry_id` → `feature_cycle`). Entry #5599. NB the *freeze* mechanism is
  whole-set-once (a set-level EXISTS guard, ADR-002), **not** per-row `ON CONFLICT` accumulate — a
  deliberate divergence to keep run identity frozen without parsing namespaces.
- **col-025** (`goal` on cycle_events) — the end-to-end hook→persist→surface precedent cloned.
  Entries #3396 (storage-on-start-row), #3399 (degrade-to-None read contract).
- **crt-036** (`retention.rs`) — the GC surface `cycle_tags` must survive.
- **crt-055 / #4178, #5051** — `SUMMARY_SCHEMA_VERSION` bump discipline + pinned test.

## Integration Surface

| Integration Point | Type / Signature | Source (verified) | Change |
|-------------------|------------------|-------------------|--------|
| `CycleParams.tags` | `tags: Option<Vec<String>>` | `mcp/tools.rs` CycleParams (~:515-542) | ADD (additive) |
| hook tag extraction | `payload["tags"] = serde_json::Value::Array` (Start only, non-empty filtered) | `uds/hook.rs:769` `build_cycle_event_or_fallthrough`, beside goal :839-880 | ADD |
| event transport | `ImplantEvent.payload` carries `tags` (JSON array of strings) | `uds/hook.rs:886`, `uds/listener.rs:2859` | reuse (payload) |
| listener read | `event.payload.get("tags").and_then(|v| v.as_array())` | `uds/listener.rs` `handle_cycle_event` :2848, step-5 spawn :3035-3080 | ADD |
| `Store::insert_cycle_event` | `(cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal: Option<&str>)` | `store/db.rs:320` | **UNCHANGED** (15 call sites preserved) |
| `Store::insert_cycle_start_with_tags` | `async fn(&self, cycle_id: &str, seq: i64, phase: Option<&str>, outcome: Option<&str>, next_phase: Option<&str>, timestamp: i64, goal: Option<&str>, tags: &[String]) -> Result<()>` | `store/db.rs` (NEW) | ADD — one BEGIN IMMEDIATE txn: cycle_start INSERT + **whole-set-once** guard `if NOT EXISTS(SELECT 1 FROM cycle_tags WHERE feature_cycle=?1) { INSERT full set } else { skip }`; returns wrote-set vs frozen-skip for the C13 trace |
| ack echo (C12) | tag phrase appended to `response_text` from `validated.cycle_type` + `params.tags` | `mcp/tools.rs` context_cycle handler, goal-ack precedent :4154-4160 | ADD (best-effort, no new interface; no `phase` read) |
| freeze trace (C13) | `tracing::info!` wrote-set / frozen-skip | `uds/listener.rs` step-5 spawn | ADD (log only) |
| `Store::get_cycle_tags` | `async fn(&self, feature_cycle: &str) -> Result<Vec<String>>` — `SELECT tag FROM cycle_tags WHERE feature_cycle=?1 ORDER BY tag` | `store/db.rs` (NEW), parity `get_cycle_start_goal` :371 | ADD |
| `cycle_tags` schema | `cycle_tags(feature_cycle TEXT NOT NULL, tag TEXT NOT NULL, PRIMARY KEY(feature_cycle, tag))` + `CREATE INDEX idx_cycle_tags_tag ON cycle_tags(tag)` | fresh: `store/db.rs::create_tables_if_needed`; migration: `store/migration.rs` `if current_version < 31` | ADD (3 paths — see ADR-001) |
| `CURRENT_SCHEMA_VERSION` | `30 → 31` | `store/migration.rs:26` | BUMP (cascade #1) |
| `RetrospectiveReport.tags` | `pub tags: Vec<String>` (required field, `#[serde(default)]`) | `unimatrix-observe/src/types.rs:382`, after `goal` :436 | ADD |
| review handler tag populate | `report.tags = store.get_cycle_tags(&feature_cycle).await.unwrap_or_default()` | `mcp/tools.rs` review, beside goal set :3409-3428 | ADD |
| `build_cycle_review_record` | serializes whole `report` (incl. `tags`) into `summary_json` | `mcp/tools.rs:4554` | reuse (automatic) |
| `render_tags_section` | `fn(&RetrospectiveReport) -> String` → `## Tags` | `mcp/response/retrospective.rs`, call after `render_goal_section` :49 | ADD |
| `SUMMARY_SCHEMA_VERSION` | `5 → 6` | `store/cycle_review_index.rs:54`; pinned test :709-716 | BUMP (cascade #2) |
| GC durability | `cycle_tags` absent from every DELETE in `gc_cycle_activity` (:116) and `gc_unattributed_activity` (:202) | `store/retention.rs` | OMIT + extend `test_gc_protected_tables_regression` :521 |
| deferred mutation seam | comment-only reservation on `context_tag` handler | `mcp/tools.rs` context_tag handler (~:1542) | comment ADD (no code) |

## Constraint (load-bearing): MCP handler does not persist

The bare `context_cycle` MCP handler is session-unaware and persists nothing (tools.rs:4062,
comment :4128). Tags persist **only** via the hook path. The `CycleParams.tags` field exists to
declare the interface (AC-06); the value that gets stored is read from `tool_input["tags"]` by the
hook (parity with how `goal` is read at hook.rs:844, not from CycleParams). There must be **no second
persistence route** (ADR-002; SR-03).

## Open Questions for the human

None blocking. Two items to acknowledge (both already decided in SCOPE, restated as risks the human
should expect at demo time):

- **SR-05 (whole-set-once, silent):** the first tag-bearing start freezes the entire set; a re-issued
  start with a *changed / new / subset / superset* set is ignored **wholesale** (EXISTS guard — never
  merged or accumulated). A tagless start does not burn the one-shot. Intended, tested, no error
  surfaced (the ADR-007 ack reports acceptance-for-recording, not the freeze outcome).
- **SR-10 (no back-fill):** cycles reviewed before deployment (summary v5) never show tags; only
  runs started after deployment carry them. Historical `## Tags` sections render empty by design.
