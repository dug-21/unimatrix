## ADR-002 vnc-047: Whole-set-once write on the hook cycle_start transaction; port entry-tag storage; no second persistence route

### Context

Cycle tags are opaque **run-identity labels** — workflow version, run mode, confidence-required, arm,
etc. (not workflow-only) — set once at cycle start (OQ-2 RESOLVED). The `context_cycle` MCP handler is
session-unaware and persists nothing (tools.rs:4062) — the only route that persists a start-time
attribute is the UDS hook path, exactly as `goal` uses it (col-025):

`hook.rs build_cycle_event_or_fallthrough` (:769, extracts goal :839-858, writes `payload["goal"]`
:877-880) → `RecordEvent{ImplantEvent}` → `listener.rs handle_cycle_event` (:2848) → fire-and-forget
spawn (:3062) → `store.insert_cycle_event(...)` (db.rs:320).

`insert_cycle_event` today acquires a single connection and runs one INSERT — **not** a multi-statement
transaction. The SCOPE requires the tag rows to be written in the **same transaction** as the
`cycle_start` event insert (atomicity). `insert_cycle_event` has ~15 call sites; changing its
signature is high-churn and risky.

**Freeze semantics are whole-set-once, not per-row accumulate.** Run identity must be frozen: an A/B
arm or workflow version stamped at the first tag-bearing start must not silently grow or drift on a
re-issued start. A per-key (per-namespace) write-once rule was rejected because it would force the
engine to parse namespaces (the `ns:` prefix), violating value-opacity (vnc-045 SD-8). So the unit of
freeze is the **entire tag set for a `feature_cycle`**, decided by whether any row already exists — the
engine never inspects tag *values* to make the decision.

### Decision

**1. Additive interface (AC-06).** `CycleParams` (tools.rs:515-542) gains `tags: Option<Vec<String>>`.
This declares the tool interface; the persisted value is read from `tool_input["tags"]` by the hook
(parity with goal, read from `tool_input` at hook.rs:844, not from `CycleParams`).

**2. Hook extraction (Start only, value-opaque).** In `build_cycle_event_or_fallthrough`, beside the
goal block, extract tags **only** when `validated.cycle_type == CycleType::Start`. Filter out
empty/whitespace-only strings (non-empty is the ONLY check — no vocabulary, allow-list, length, charset,
or namespace parsing; value-opacity parity vnc-045 SD-8). If any tag survives, set
`payload["tags"] = serde_json::Value::Array([...])`; if none survive, omit the key. The hook runs
outside tokio and must never fail — extraction is infallible. PhaseEnd/Stop never extract tags.

**3. New transactional store primitive** (db.rs), leaving `insert_cycle_event` untouched:

```rust
pub async fn insert_cycle_start_with_tags(
    &self, cycle_id: &str, seq: i64,
    phase: Option<&str>, outcome: Option<&str>, next_phase: Option<&str>,
    timestamp: i64, goal: Option<&str>, tags: &[String],
) -> Result<()> {
    // BEGIN IMMEDIATE: take the write lock up front so the EXISTS check below is
    // race-safe against a concurrent start for the same feature_cycle.
    let mut txn = /* write_pool, immediate */;
    // (a) same INSERT statement as insert_cycle_event, event_type = "cycle_start"
    // (b) WHOLE-SET-ONCE guard:
    //     if NOT EXISTS(SELECT 1 FROM cycle_tags WHERE feature_cycle = ?1) {
    //         INSERT every submitted tag: cycle_tags(feature_cycle, tag)
    //     } else {
    //         skip the entire tag write — the set is frozen
    //     }
    txn.commit().await?;
}
```

The freeze mechanism is the **EXISTS guard**, not per-row conflict handling. The PK
`(feature_cycle, tag)` is retained purely for integrity; `BEGIN IMMEDIATE` (not a row-level
`ON CONFLICT`) is what makes the set-freeze atomic under a concurrent start.

**Precise rule:** the first tag-bearing `cycle_start` locks the whole set; every later start — with the
same, subset, superset, or different tags — is a **wholesale no-op on tags** (never merged, never
accumulated). A **tagless start never burns the one-shot** (EXISTS stays false until a non-empty set
lands, so a later tag-bearing start can still set it). The `cycle_start` event *row* is still inserted
for every start (parity with goal, seq++); only the tag write is frozen.

**4. Listener routing.** In `handle_cycle_event` step-5 spawn (listener.rs:3062), read
`event.payload.get("tags")` as an array of strings. Then:
- `if lifecycle == Start && !tags.is_empty()` → `insert_cycle_start_with_tags(...)`
- `else` → `insert_cycle_event(...)` (UNCHANGED — all other events, and start-without-tags)

Gate on the **same** `!feature_cycle.is_empty()` condition that already guards the cycle_event insert
(ADR-003). Do **not** gate on `attribution_result`.

**No second persistence route** (SR-03): the MCP handler stays non-persisting; no alternate write path,
no "did my tags persist?" API. Set-and-forget. (The best-effort ack echo added in ADR-007 is
accept-for-recording only, not a durable confirm.)

### Consequences

- Easier: `insert_cycle_event`'s 15 call sites are untouched; blast radius is one new method + one
  listener branch.
- Easier: atomicity is structural — cycle_start row and its tags commit together or not at all.
- Easier: run identity is frozen by construction — a re-issued start cannot grow, shrink, or alter the
  recorded set, protecting A/B/analysis trust.
- Easier: value-opacity preserved — the freeze decision reads only *existence* of rows, never tag
  values or namespaces.
- Harder (accepted, SR-05): once the set is non-empty, a re-issued start with **differing / new /
  subset / superset** tags is ignored **wholesale** — not merged, not accumulated. This must be covered
  by an explicit test (submit set A, re-issue with set B, assert stored == A).
- Harder: the whole-set-once branch (EXISTS guard) plus the tagless/tag-bearing ordering must be
  exercised on the assembled path (MCP-start → hook → listener → cycle_review), not via a direct store
  insert (SR-08).
- The absent/evicted-session durability envelope is delegated to ADR-003.
- Cross-ref ADR-001 (table), ADR-003 (durability envelope), ADR-006 (deferred mutation home),
  ADR-007 (ack echo). Prerequisite edge → #5599: `cycle_tags` storage ports the entry-tag junction
  model; diverging forks the tag model (SR-06).
