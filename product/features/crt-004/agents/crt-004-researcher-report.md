# Agent Report: crt-004-researcher

## Key Findings

1. **Raw co-access data already exists in AUDIT_LOG.** Every retrieval logs `target_ids: Vec<u64>`. However, AUDIT_LOG is append-only and not indexed for efficient pairwise lookup. A dedicated CO_ACCESS table is needed.

2. **Natural integration point identified.** `record_usage_for_entries` in `server.rs` receives the full entry ID list for every retrieval. Co-access pair generation plugs in here, after the existing usage recording, using the same fire-and-forget `spawn_blocking` pattern.

3. **UsageDedup is extensible.** The existing `UsageDedup` struct (Mutex-protected HashSet+HashMap) can accommodate co-access pair dedup by adding a `HashSet<(u64, u64)>` for ordered pairs.

4. **Confidence formula modification is non-trivial.** The six-factor additive composite with weights summing to 1.0 was carefully designed in crt-002. Adding a seventh factor requires redistributing all weights, which affects every confidence computation in the system. The function pointer pattern (`Option<&dyn Fn(&EntryRecord, u64) -> f32>`) also doesn't provide access to co-access data (relational, not per-entry).

5. **Search re-ranking has a clean extension point.** After step 9b in `context_search` (similarity+confidence re-rank), a co-access boost step can be added as step 9c. The sorted results provide natural "anchor" entries for co-access lookup.

6. **Scale is comfortable.** At 2000 entries max, theoretical pair space is ~2M but actual active pairs will be 1-2 orders of magnitude smaller. Storage overhead is negligible (~280KB at 10K pairs).

7. **No schema migration needed for the core feature.** CO_ACCESS is a new table, not a change to EntryRecord. The optional co-access affinity confidence factor may require an EntryRecord field, but this is a design decision for Phase 2.

## Scope Boundaries

- **In scope:** CO_ACCESS table, co-access recording in usage pipeline, session dedup, search boost, status reporting, staleness decay.
- **Out of scope:** Graph algorithms, transitive relationships, cross-session co-access, new tools, UI, background jobs.
- **Design decision deferred to Phase 2:** Whether co-access affinity lives on EntryRecord (stored) or is computed at query time (ephemeral).

## Risks Identified

- Confidence weight redistribution could degrade quality of existing entries
- Quadratic pair generation needs bounding to prevent write amplification
- Co-access boost could create feedback loops (boosted entries get retrieved more, generating more co-access, getting boosted more)

## Open Questions

See SCOPE.md Open Questions section.
