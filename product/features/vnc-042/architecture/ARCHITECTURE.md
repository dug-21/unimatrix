# vnc-042 Architecture — `context_get` supersession resolution

## System Overview

`context_get` is the most-used read tool on the MCP surface. Today it is a raw by-ID
read (`tools.rs:978` → `self.entry_store.get(id)`) with no status check and no
supersession follow, so a durable id pointing at a **deprecated** entry silently returns
stale content. vnc-042 makes `context_get` resolve a requested deprecated id to its
**active terminal by default**, reusing the existing supersession-walk primitive.

This is a **surgical single-tool contract change** confined to the MCP server crate. No
schema, no new SQL, no changes to other tools (`context_search`, `context_lookup`,
`context_graph` are untouched — NG-3/NG-4/NG-5). The resolution capability already exists
(`follow_to_current`, `query_current_terminal`); `context_get` simply routes to it.

## Component Breakdown

| Component | File | Responsibility | Change |
|-----------|------|----------------|--------|
| `GetParams` | `tools.rs:246-274` | Deserialize tool params | **Add** `follow_supersessions: Option<bool>`, `#[serde(default)]` |
| `context_get` handler | `tools.rs:950-1052` | Orchestrate fetch → edges → format | **Add** resolution branch before fetch; select effective id; choose response formatter |
| `follow_to_current` | `graph_read_neighbors.rs:36-55` | Walk `superseded_by` to Active terminal, 50-hop cap | **Reuse** (widen visibility to `pub(crate)` + re-export) |
| `build_edges_view` | `get_edges.rs` | Assemble ranked depth-1 edges for an id | **Reuse**, keyed on the *effective* (resolved) id |
| response formatter | `response/entries.rs` | Render `EntryRecord` (+edges) to `CallToolResult` | **Add** a note-carrying variant; `format_single_entry` stays byte-identical |
| tool description | `tools.rs:947-948` | Agent-facing contract text | **Update** to document new default + escape hatch (C-5) |

## Component Interactions / Data Flow

```
context_get(id, follow_supersessions?, include_edges?, format)
  │
  ├─ validate → id: u64  (validated_id, tools.rs:977)
  │
  ├─ resolution branch (NEW):
  │    follow_supersessions == Some(false)   → effective_id = id;  mode = AsStored
  │    None | Some(true) (DEFAULT):
  │        follow_to_current(&self.store, id).await
  │          Some(t) && t == id   → effective_id = id;  mode = CleanPassthrough
  │          Some(t) && t != id   → effective_id = t;   mode = Followed{from:id,to:t}
  │          None                 → effective_id = id;  mode = DeadEnd{requested:id}   (ADR-002)
  │
  ├─ fetch:  entry = entry_store.get(effective_id)          (single fetch, no double-read)
  │    AsStored mode: if entry deprecated → mode = AsStoredDeprecated{superseded_by}
  │
  ├─ edges:  include_edges ≠ Some(false) → build_edges_view(&self.store, effective_id)   (ADR-003 / SR-03)
  │
  └─ format: mode == CleanPassthrough → format_single_entry(...)          (unchanged, byte-identical)
             else                     → format_single_entry_with_note(..., ResolutionNote)  (ADR-003)
```

The handler owns all resolution logic; the formatter only *renders* whatever entry the
handler selected. `follow_supersessions`, `format`, and `include_edges` are orthogonal:
resolution picks *which* entry, `include_edges` decides *whether* edges are surfaced,
`format` decides *how* it renders (AC-07).

## Technology / Design Decisions (ADRs)

| ADR | Ruling |
|-----|--------|
| ADR-001 | Parameter is `follow_supersessions: Option<bool>`, `#[serde(default)]`, **default `true`**. The default divergence from `context_graph`'s `resolve_supersessions: bool = false` is **accepted** (distinct verb `follow_*` signals the flipped default; shared noun `supersessions`); no forced cross-tool standardization now. |
| ADR-002 | On a dead-end chain (`follow_to_current → None`: orphaned/quarantined terminal or >50 hops), return the **originally-requested** entry with a loud "no active successor" flag — no new walk, no stop-id surfacing (AC-4/AC-5, SR-05). |
| ADR-003 | The resolution notice / dead-end flag / deprecated footer is attached in a **handler-side formatter variant** (`format_single_entry_with_note`), never inside `format_single_entry` (preserves the byte-identity invariant, SR-04). Resolved gets rebuild edges on the **resolved terminal id** (SR-03). Under `format="json"` the message renders as a **structured `resolution` object**, present only when non-clean (OQ-3). |

## Integration Points / Dependencies

- **`follow_to_current`** (`graph_read_neighbors.rs:36-55`) — canonical production copy.
  Currently `pub(super)` (reachable only within the `graph_read` module tree). **Must be
  widened to `pub(crate)` and re-exported from `graph_read.rs`** so the `tools.rs` handler
  can call it via the already-established fully-qualified path
  (`crate::mcp::graph_read::...`, Pattern #4436 — the handler already calls
  `crate::mcp::graph_read::handle_graph`). This is the only visibility change required.
  *Do not use* `handle_current` (`graph_read_supersession.rs:86-103`) — it errors on
  orphaned terminals, violating AC-4.
- **Duplication flag (not fixed here):** `follow_to_current` exists twice — the canonical
  copy at `graph_read_neighbors.rs:36` and a second at `graph_read_supersession.rs:122`.
  vnc-042 calls the canonical copy per spawn directive and does not consolidate the copies
  (out of scope). Flagged for a future cleanup.
- **`build_edges_view`** (`get_edges.rs`) — unchanged; called with the effective id.
- **`supersedes`/`superseded_by` columns** (`schema.rs`, written by `context_correct`) —
  read-only consumers here; no schema change (NG-5).

## Integration Surface

| Integration Point | Type / Signature | Source | vnc-042 action |
|-------------------|------------------|--------|----------------|
| `GetParams.follow_supersessions` | `Option<bool>`, `#[serde(default)]` | `tools.rs:246-274` | ADD (mirrors `include_edges` three-state serde) |
| `follow_to_current` | `async fn(store: &Store, id: u64) -> Option<u64>` | `graph_read_neighbors.rs:36` | REUSE; widen to `pub(crate)` + re-export from `graph_read.rs` |
| handler `id` | `u64` (from `validated_id(params.id) -> Result<u64, _>`, `validation.rs:81`) | `tools.rs:977` | feeds `follow_to_current` directly — no cast |
| `build_edges_view` | `async fn(store, id) -> Result<EdgesView, StoreError>` | `get_edges.rs`, called `tools.rs:991` | REUSE; pass `effective_id` not `id` |
| `format_single_entry` | `fn(&EntryRecord, ResponseFormat, Option<&EdgesView>) -> CallToolResult` | `response/entries.rs:24` | UNCHANGED (byte-identity invariant, ADR-003 vnc-038 test at `response/mod.rs:367`) |
| `format_single_entry_with_note` (NEW) | `fn(&EntryRecord, ResponseFormat, Option<&EdgesView>, note: &ResolutionNote) -> CallToolResult` | `response/entries.rs` (new) | ADD; mirrors `format_store_success_with_note` (`response/entries.rs:189`) |
| tool description string | literal | `tools.rs:947-948` | UPDATE (C-5) |

### `ResolutionNote` shape (handler → formatter)

Handler passes a small enum describing what happened; the formatter renders it per format.

| Variant | Trigger (AC) | Text formats (summary/markdown) | JSON (`resolution` object) |
|---------|--------------|----------------------------------|-----------------------------|
| `Followed { from, to }` | hop occurred (AC-02) | prepend `↻ Requested #{from} (deprecated) → returning current version #{to}.` | `{"status":"followed","requested_id":from,"returned_id":to}` |
| `DeadEnd { requested }` | `None` terminal (AC-04) | prepend loud `⚠ Requested #{requested}: no active successor found (chain dead-ends on a non-active entry).` | `{"status":"no_active_successor","requested_id":requested}` |
| `AsStoredDeprecated { requested, superseded_by }` | escape hatch on deprecated (AC-03) | append footer `deprecated; superseded by #{superseded_by} (omit follow_supersessions to follow).` | `{"status":"as_stored_deprecated","requested_id":requested,"superseded_by":superseded_by}` |
| (none) | clean passthrough / active as-stored (AC-02) | — uses `format_single_entry`, no note | — no `resolution` key (byte-identity preserved) |

JSON emits the `resolution` object **only** in the three non-clean cases, so the common
active-entry response is byte-identical to today (SR-02a canary + ~15 shape tests stay
green). Programmatic callers branch on `resolution.status` — a stable typed contract, not
a parsed string.

## Error Boundaries (C-4, FAIL-LOUD)

- Primary fetch failure (`entry_store.get(effective_id)`) → mapped `ServerError::Core` and
  returned, exactly as today.
- `build_edges_view` failure → FAIL-LOUD, same mapping as the primary read (existing FR-19
  behavior, `tools.rs:984-987`); resolution does not soften this.
- `follow_to_current` internal store error → the helper returns `None`; the handler treats
  it as the dead-end path (ADR-002) and fails loud with the flag — never silent/empty (AC-4).
- No `.unwrap()` in non-test code; the 50-hop cap and `status=0` terminal guard inside the
  reused primitives are load-bearing and untouched (C-3, #4538).

## Test Blast Radius (SR-02 — for spec/tester acceptance-map rows)

- **Byte-identity canary** `test_none_json_byte_identical_to_base_object`
  (`response/mod.rs:367`) — guarded by keeping notice OUT of `format_single_entry`; clean
  passthrough must produce the identical CallToolResult.
- **~15 `format_single_entry` shape tests** (`response/mod.rs`) — unchanged path; new
  variant gets its own coverage.
- **`include_edges` contract + additivity** (`tools.rs:5630-5690`, incl.
  `test_get_params_no_existing_field_removed_or_retyped` NFR-4) — new field is additive.
- **~18 `get_edges_tests.rs`** — assert edges-of-queried-id; resolved-terminal edge keying
  (ADR-003) needs review for the resolved case.
- No fixtures/goldens encode get responses (verified SR-02); JS/E2E assert transport
  framing only.

## Open Questions (for spec / human)

- **OQ-3 rendered, one sub-choice for spec:** ADR-003 pins JSON as a structured
  `resolution` object. If the human prefers exact `format_store_success_with_note`
  parity (a flat `"note"` string) over a typed discriminant, that is the only remaining
  toggle — flagged, recommendation is the structured object for programmatic stability.
- **NG-1 asymmetry (SR-07):** a resolved get returns the terminal's edge *list* but the
  edge *targets* inside it remain unresolved (old id+title). Accepted; the resolution
  notice makes the asymmetry legible. Neighbor-target resolution is the deferred follow-up.
