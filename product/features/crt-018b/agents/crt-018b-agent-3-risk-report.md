# Agent Report: crt-018b-agent-3-risk

## Output

- `/workspaces/unimatrix/product/features/crt-018b/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Risk Count |
|----------|-----------|
| Critical | 4 |
| High | 4 |
| Medium | 6 |
| Low | 2 |
| **Total** | **14** |

## Top Risks for Human Attention

**R-13 (Critical)** — Write lock on `EffectivenessState` held during auto-quarantine SQL write. The architecture specifies the lock must be released before SQL (NFR-02), but an implementation that checks threshold and immediately calls `quarantine_entry()` while still inside the write guard would block all concurrent searches for the duration of every SQLite write. Under bulk quarantine this is a latency regression for every active query. Requires explicit code review of lock scope boundaries in `maintenance_tick()`.

**R-01 (Critical)** — ADR-001 introduces a two-lock sequence (`effectiveness_state.read()` then `cached_snapshot.lock()`). The ADR explicitly notes this as a potential deadlock surface and specifies the read lock must be released before the mutex is acquired. An implementation that holds both simultaneously will deadlock on the first search call after a tick fires. Requires a lock-ordering test and careful scoping.

**R-02 (Critical)** — The utility delta must be applied at all four `rerank_score` call sites in `search.rs` (Steps 7 and 8, both sort passes). Missing even one call site creates asymmetric ranking between the initial sort and the co-access re-sort, producing non-deterministic ordering changes visible to agents. AC-05 and AC-04 together require all four sites.

**R-06 (High)** — `SearchService` and `BriefingService` are both `Clone` (rmcp requirement). ADR-001 specifies the generation-cache fields must use `Arc<Mutex<_>>` so all clones share a single cached snapshot. A plain (non-Arc) field would cause each clone to maintain an independent stale copy, producing divergent search orderings across concurrent requests.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — findings: #1366, #1542, #732 (tick loop error recovery patterns). Informed R-08 and R-04 severity.
- Queried: `/uni-knowledge-search` for "risk pattern" category:pattern — findings: double-lock deadlock pattern (R-01), write-lock-held-during-SQL (R-13).
- Queried: `/uni-knowledge-search` for "ConfidenceState Arc RwLock" — findings: #255, #1480. Informed R-06.
- Stored: nothing novel — R-01 and R-13 patterns already exist in Unimatrix knowledge base. R-06 (generation cache not shared across rmcp clones) is feature-specific and not yet a confirmed multi-feature pattern; will store if it recurs in crt-019+ delivery.
