## ADR-002: Explicit NULL Topic and Feature Cycle Handling

### Context

SR-06 identifies that NULL `feature_cycle` in sessions has caused silent downstream failures before (Unimatrix #981, lesson-learned #756). Entries with NULL or empty `topic` and sessions with NULL or empty `feature_cycle` will produce incorrect effectiveness classifications if silently dropped.

The effectiveness classifier needs to determine whether an entry's topic is "active" (has recent sessions). If the entry's topic is empty/NULL, or the session's feature_cycle is empty/NULL, the join between these concepts breaks.

Three approaches:
- A) Silently exclude NULL/empty values from all queries (risk: entries with no topic are never classified, silent data loss)
- B) Map NULL/empty to a sentinel value `"(unattributed)"` and treat as a distinct bucket (explicit, visible, no data loss)
- C) Treat NULL topic entries as always "active topic" (over-classifies, but no data loss)

### Decision

**Option B: Map NULL/empty to `"(unattributed)"` sentinel.**

For entries:
- Entries with empty or NULL `topic` field are classified with `topic = "(unattributed)"`
- The `"(unattributed)"` pseudo-topic is considered **inactive** (no sessions can match it) unless sessions also have NULL/empty feature_cycle, in which case those sessions are grouped under `"(unattributed)"` too
- This makes unattributed entries visible in the output (they appear as Unmatched or Settled) rather than silently disappearing

For sessions:
- Sessions with NULL or empty `feature_cycle` are **excluded** from the active_topics set (they cannot meaningfully indicate topic activity)
- However, for injection stats, sessions with NULL feature_cycle ARE included in the JOIN — we still count their outcomes against injected entries. The NULL only affects topic activity detection, not per-entry outcome counting.

SQL implementation:
```sql
-- Active topics excludes NULL/empty
SELECT DISTINCT feature_cycle FROM sessions
WHERE feature_cycle IS NOT NULL AND feature_cycle != ''

-- Injection stats includes all sessions with outcomes (regardless of feature_cycle)
SELECT il.entry_id, COUNT(DISTINCT il.session_id), ...
FROM injection_log il JOIN sessions s ON il.session_id = s.session_id
WHERE s.outcome IS NOT NULL
GROUP BY il.entry_id
```

Engine implementation:
```rust
let topic = if entry_topic.is_empty() { "(unattributed)" } else { entry_topic };
```

### Consequences

- No silent data loss — every active entry gets classified
- Unattributed entries surface in effectiveness output for human review
- Sessions with NULL feature_cycle still contribute to injection/outcome counts (correct: the session outcome is real signal even without topic attribution)
- The `"(unattributed)"` bucket in per-source tables makes NULL data visible to operators
- Matches the existing pattern in `compute_status_aggregates()` which maps empty trust_source to `"(none)"`
