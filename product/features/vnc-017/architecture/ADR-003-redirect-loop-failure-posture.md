## ADR-003: Redirect Loop Failure Posture — Warn and Continue on All Error Conditions

### Context

The redirect loop in `context_correct` encounters three failure conditions:

**SR-06 — Quarantined/deprecated source (Contradicts-specific risk)**
`redirect_graph_edge`'s Contradicts path writes 4 rows atomically: it deletes the old
bidirectional pair and inserts a new pair from source→new_target AND new_target→source.
The caller contract requires the caller to validate before calling; the function does not
re-validate. If the source entry is quarantined or deprecated, the 4-row transaction
would insert a new edge originating from an invalid source, violating graph integrity
invariants with no error returned.

**SR-02 — Redirect conflict (already-redirected edge)**
`redirect_graph_edge` uses `INSERT OR IGNORE` on the new-direction inserts. If the edge
`source → new_target` already exists (edge was previously redirected by a manual
`context_edge(mode="redirect")` call), the INSERT is silently ignored and the function
returns `Ok(())`. This is not an error condition. Contrast with `write_graph_edge` which
returns `bool` (true=inserted, false=conflict-ignored); `redirect_graph_edge` returns
`Result<(), EdgeRedirectError>` — conflicts are invisible to the caller and are already
correctly handled inside the function.

**SQL infrastructure error**
`redirect_graph_edge` returns `Err(EdgeRedirectError::TransactionError(e))` on SQLite
pool or transaction failure. These are infrastructure errors, not logical failures.

Three posture options were considered:
1. Abort the entire `context_correct` call on any redirect failure.
2. Wrap all redirects in a single transaction that rolls back on any failure.
3. Log and continue per ADR-003 (vnc-015) partial-write posture.

### Decision

**For SR-06 (invalid source):** Before calling `redirect_graph_edge`, check the source
entry status via `store.get(source_id)`. If the status is `Quarantined` or `Deprecated`,
emit `tracing::warn!` with the source_id and relation_type, increment the skip counter,
and continue to the next edge. Do not call `redirect_graph_edge` for invalid sources.

This decision is consistent with ADR-003 (vnc-015): partial-write posture is accepted.
The recommendation from pattern #4459 (Unimatrix) is followed explicitly.

**For SQL errors:** Emit `tracing::warn!` with the error and edge details, increment the
failed counter, and continue. The correction always succeeds if `correct_entry` succeeds.

**For conflicts (SR-02):** `redirect_graph_edge` returns `Ok(())` — the loop treats this
as success, increments the redirected counter. No special handling needed.

Rationale:
1. Aborting `context_correct` for redirect failures would introduce a new failure mode
   where a valid knowledge correction is rejected due to a graph maintenance side effect —
   this is worse than partial redirect state.
2. A single wrapping transaction would require refactoring `redirect_graph_edge` to accept
   `&mut Transaction` — out of scope and would change the RAII contract (lesson #2269).
3. The ADR-003 blast-radius posture is already established for this handler; extending it
   to auto-redirect is consistent with existing architecture.
4. Source validation before calling `redirect_graph_edge` is a strict improvement over
   silently writing edges from invalid sources — it prevents a correctness defect
   (quarantined-source edges) without changing the overall warn+continue posture.

### Consequences

Easier: correction always succeeds when the entry operation succeeds; no new failure
modes introduced; partial redirect is a known degraded state, not a corrupt state; the
`DependencyOnDeprecated` detection rule will surface unredirected edges on the next tick
if any fail.

Harder: agents cannot distinguish "all edges redirected" from "some edges skipped" from
the call response alone (only the summary count line distinguishes them). Server-side
logs provide full detail. If SR-06 sources are common, the skip count in the response
text provides visibility.

**Return contract for redirect_graph_edge (explicit, per SR-02 spec discipline):**

| Return value | Meaning | Loop action |
|---|---|---|
| `Ok(())` | Edge inserted (or UNIQUE conflict idempotently ignored) | redirected++ |
| `Err(EdgeRedirectError::TransactionError(e))` | SQL infrastructure failure | warn, failed++ |
| `Err(EdgeRedirectError::TargetNotFound { .. })` | Should not occur — target is new entry | warn, failed++ |
| `Err(EdgeRedirectError::TargetQuarantined { .. })` | Should not occur — target is new active entry | warn, failed++ |

The TargetNotFound and TargetQuarantined variants cannot occur in practice (the redirect
target is `correct_result.corrected_entry.id`, an Active entry just inserted), but the
loop handles them as warn+failed for defensive completeness.
