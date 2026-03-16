# Retro Architect Report: bugfix-279

Agent ID: bugfix-279-retro-architect

---

## 1. Patterns

### New: #1758 — Extract spawn_blocking body into named sync helper for unit testability

**Stored.** Category: pattern. Topic: unimatrix-server.

The technique applied in this bugfix — factoring `fetch_observation_batch()` out of the `spawn_blocking` closure and into a named synchronous `fn` — is not represented anywhere in Unimatrix. The closest existing entries are:

- #1366 (Tick Loop Error Recovery: Extract-and-Catch): extracts the *async* loop body into an async fn for resilience. Orthogonal — that pattern is about error isolation in the async layer, not about the sync payload inside spawn_blocking.
- #731 (Batched Fire-and-Forget DB Writes): about batching write calls to reduce pool saturation. Does not address testability of spawn_blocking bodies.

The helper extraction technique is reusable: any spawn_blocking closure that contains non-trivial logic (SQL + deserialization, computation) benefits from extraction into a named sync fn for the same reason — direct `#[test]` coverage without Tokio runtime or TestHarness. The six tests added in bugfix-279 cover batch capping, watermark advancement, remainder, empty store, no-reprocessing, and constant value — all via simple `tempfile::TempDir` Store setup.

### Skipped: No updates to existing pattern entries

No existing pattern entries required modification. #1366 and #731 remain accurate and were not superseded.

---

## 2. Procedures

No procedure gap identified. The bugfix followed the standard investigator → fix → verify → gate flow without friction. The helper extraction technique is structural (belongs as a pattern, not a procedure step). No procedure entries created or updated.

---

## 3. ADR Status

No ADRs were created during bugfix-279. The batch size value (1000) is a tuning constant, not an architectural decision requiring an ADR. Nothing to validate.

---

## 4. Lessons

### #1736 assessment

**Corrected → #1759.**

#1736 had three weaknesses:

1. **Root cause mechanism was incomplete.** The entry explained the LIMIT value as the control variable but did not state *why* the mutex hold is bounded by the batch size — namely that `lock_conn()` returns a `MutexGuard` that lives for the full closure lifetime, making every row's deserialization a hold extension. This is the key fact that makes the pattern generalizable.

2. **No explicit actionable takeaway.** The entry described the problem and the fix approach but did not state the rule: "Any spawn_blocking closure iterating a variable-length result set from a mutex-protected connection MUST express its LIMIT as a named constant."

3. **Shipped value mismatch.** The investigator recommended 500; the implementation shipped 1000 with no documented rationale. The entry recorded "safe reduction: 10000 → 500" as if that were what shipped. Corrected to reflect the actual shipped value and the reasoning (reduce tick count needed to clear backlog while still providing 10x hold-time reduction).

4. **Missing cross-reference.** The companion technique — extracting the query body into a named sync helper to make the batch cap unit-testable — was not referenced. The corrected entry points to #1758.

**#1736 is deprecated. #1759 is the authoritative entry.**

### No new lesson entries

The main lesson is fully covered by #1759. No duplicate or additive lesson warranted.

---

## Summary Table

| Type | ID | Action | Reason |
|------|----|--------|--------|
| Pattern | #1758 | NEW | Helper extraction into named sync fn for spawn_blocking testability — novel, not in Unimatrix |
| Lesson | #1759 | CORRECTED from #1736 | Added root-cause mechanism, actionable takeaway, shipped value, cross-reference to #1758 |
| Pattern | #1366, #731 | SKIPPED | Existing entries remain accurate; not superseded |
| Procedure | — | NONE | No procedure gap found |
| ADR | — | NONE | No ADRs created during this bugfix |
