# Component 1 — `context_get` handler + `GetParams` + tool description

**File:** `crates/unimatrix-server/src/mcp/tools.rs`
**Regions:** `GetParams` (`:246-274`), `context_get` handler (`:950-1052`), tool-desc (`:947-948`)

## Purpose

Own all resolution logic. Given the requested id and `follow_supersessions`, select the
`effective_id` and an optional `ResolutionNote`, fetch the effective entry once, build its
edges on the same id, and route to the base or note-carrying formatter. The formatter only
renders; the handler decides.

## 1a. `GetParams` — ADD one field (purely additive, NFR-06)

```
struct GetParams {
    ... existing fields UNCHANGED (id, agent_id, format, session_id, feature, helpful, include_edges) ...

    /// Resolve a requested deprecated id to its active terminal (vnc-042).
    /// - None (omitted) / Some(true) => DEFAULT-ON: follow superseded_by to the Active terminal.
    /// - Some(false) => escape hatch: return the entry exactly as stored (any status).
    #[serde(default)]
    follow_supersessions: Option<bool>      // Option<bool>, NOT bare bool
}
```

Constraints:
- MUST be `Option<bool>` with `#[serde(default)]`. A bare `#[serde(default)] bool` resolves to
  `bool::default() == false` = default-OFF and silently inverts AC-06 — FORBIDDEN (R-02, C-2, FR-01).
- Plain `Option<bool>`; do NOT wrap in any `deserialize_*_or_string` coercion (NFR-02, #3728).
- Mirrors the existing `include_edges: Option<bool>` three-state shape exactly.

## 1b. Tool description strings — UPDATE (C-5, FR-13, R-09 proxy)

Extend the `#[tool(description = ...)]` text (and the `GetParams` field doc that the schema
surfaces) to state:
- Default resolves a deprecated id to its current (active terminal) version.
- `follow_supersessions=false` is the escape hatch returning the entry exactly as stored
  (audit / lookback / provenance).

A description that lies to agents is a known hazard (#4303). Do not describe the old raw-read
behavior.

## 1c. Handler control flow — resolution branch BEFORE fetch

Replace the current straight-line "get entry (`:977-980`) → edges (`:988-997`) → format
(`:1000`)" with: resolve → fetch(effective) → edges(effective) → route. Audit/usage/session
recording (`:1002-1049`) stays UNCHANGED except it logs `effective_id` (see note below).

```
// steps 1-2 (phase snapshot, ctx, caps, validation) UNCHANGED

// --- step 3: RESOLUTION BRANCH (NEW) -----------------------------------------
id = validated_id(params.id)?                 // u64, no cast (feeds follow_to_current directly)

(effective_id, note) = match params.follow_supersessions {

    Some(false) =>                             // escape hatch — AsStored, no walk (ADR-001)
        (id, PENDING_AS_STORED)                // note finalized AFTER fetch (needs entry.status)

    None | Some(true) =>                       // DEFAULT-ON — handler owns default (C-2)
        match follow_to_current(&self.store, id).await {   // Component 3, canonical copy
            Some(t) if t == id => (id, None)                       // CleanPassthrough — no note (FR-05)
            Some(t)            => (t,  Some(Followed{ from: id, to: t }))   // hop (FR-04, AC-02)
            None               => (id, Some(DeadEnd{ requested: id }))     // dead-end (ADR-002, FR-08)
        }
}

// --- step 4: SINGLE FETCH on effective_id ------------------------------------
entry = self.entry_store.get(effective_id).await
            .map_err(|e| ServerError::Core(CoreError::Store(e)))?   // FAIL-LOUD, unchanged mapping (C-4)

// finalize the escape-hatch note now that we have entry.status (FR-07 / AC-03 / R-08)
if note == PENDING_AS_STORED {
    note = if entry.status == Status::Deprecated {
               Some(AsStoredDeprecated{ requested: id, superseded_by: entry.superseded_by })
           } else {
               None                            // active / proposed / quarantined as-stored => no footer
           }
}

// --- step 5: EDGES on the SAME effective_id (ADR-003 / SR-03 / R-03) ----------
edges_view = match params.include_edges {
    Some(false)        => None,
    None | Some(true)  => Some(
        build_edges_view(&self.store, effective_id).await         // effective_id, NOT id
            .map_err(|e| ServerError::Core(CoreError::Store(e)))?  // FAIL-LOUD, same mapping (FR-14)
    )
}

// --- step 6: FORMAT ROUTE (ADR-003 / C-7) ------------------------------------
result = match note {
    None       => format_single_entry(&entry, ctx.format, edges_view.as_ref())          // byte-identical
    Some(n)    => format_single_entry_with_note(&entry, ctx.format, edges_view.as_ref(), &n)
}

// steps 7-9 (audit, usage, session record) UNCHANGED except use effective_id — see note
Ok(result)
```

### Branch truth table (the four control-flow outcomes)

| `follow_supersessions` | `follow_to_current` | `effective_id` | note | formatter |
|------------------------|---------------------|----------------|------|-----------|
| `Some(false)`, entry Deprecated | (not called) | `id` | `AsStoredDeprecated{id, superseded_by}` | `_with_note` |
| `Some(false)`, entry not Deprecated | (not called) | `id` | `None` | base |
| `None`/`Some(true)` | `Some(t==id)` | `id` | `None` (clean) | base |
| `None`/`Some(true)` | `Some(t!=id)` | `t` | `Followed{id, t}` | `_with_note` |
| `None`/`Some(true)` | `None` | `id` | `DeadEnd{id}` | `_with_note` |

## Deprecated predicate

Footer applies only when the fetched as-stored entry has `status == Status::Deprecated`
(`schema.rs:12`). `superseded_by` may be `Some(z)` (normal correction) or `None`
(orphaned/quarantined-origin deprecated → R-08 pointerless footer). Quarantined-*status*
requested entries (status 3) are returned verbatim with NO footer — footer is deprecated-only
per FR-07 (see Open Questions).

## Initialization / State

No new state, no constructor change, no new fields on the tool struct. `follow_to_current`,
`entry_store`, and `self.store` already exist on the handler receiver.

## Data Flow

- **Inputs:** `params.id` (untrusted i64→u64 via `validated_id`), `params.follow_supersessions`
  (untrusted `Option<bool>`), `params.include_edges`, `params.format`.
- **Internal:** `id: u64` → `follow_to_current` (no cast) → `effective_id: u64` → `entry_store.get`
  + `build_edges_view` (both on `effective_id`) → `ResolutionNote` → formatter.
- **Output:** `CallToolResult` (same outer shape as today).

## Error Handling (C-4, FAIL-LOUD)

- `validated_id` error → returned (unchanged).
- `entry_store.get(effective_id)` error → `ServerError::Core(CoreError::Store)`, returned
  (unchanged mapping). Covers terminal-fetch race (terminal deleted between walk and fetch)
  → FAIL-LOUD, NOT a dead-end flag.
- `build_edges_view` error → same mapping, returned (FR-14). Resolution does not soften it.
- `follow_to_current` internal store error → helper returns `None` → dead-end path → loud
  `DeadEnd` flag, never silent/empty (ADR-002, R-04).
- No `.unwrap()` / `.expect()` in this handler. 50-hop cap + `status=0` guard live inside
  Component 3 and are untouched (C-3).

## Audit / usage note

`target_ids`, usage `record_access`, and `record_confirmed_entry` currently use `id`. They
should record the **effective_id** actually returned (the entry the caller received), or record
both. Minimal-change option: keep `id` for the audit `detail` string but pass `effective_id`
to `target_ids`/usage so access accounting lands on the returned entry. FLAG: confirm desired
audit semantics with reviewers — not covered by an AC, low risk, but a behavior choice.

## Key Test Scenarios (hints; authoritative plan in test-plan/)

- Default (field absent) on deprecated A→B(active) → returns B, `Followed{A,B}` notice (AC-01/02/06, **behavioral** default-on, R-02).
- Clean passthrough: `context_get(B)` (B active terminal) → no notice, base formatter (AC-02, R-01).
- `follow_supersessions=false` on deprecated A with `superseded_by=Some(B)` → A verbatim + footer naming #B (AC-03).
- `follow_supersessions=false` on orphaned deprecated (`superseded_by=None`) → A verbatim + pointerless footer, no `#{}`/panic (AC-08/R-08).
- Dead-end: chain ending on orphaned/quarantined terminal, and >50-hop, and self-cycle → non-empty, `DeadEnd{id}` flag, returned id == requested id (AC-04/R-04).
- Orthogonality matrix: A→B across `format ∈ {null, markdown, json}` × `include_edges ∈ {omit, true, false}` → always resolves to B; edges keyed on B; byte-identity canary unaffected on `format=null` clean (AC-07).
- `effective_id` threading: hopped get with `include_edges=true` → edges are **B's** (R-03).
- Field-present `true`→follow, `false`→as-stored; quoted-scalar must not coerce (NFR-02).
