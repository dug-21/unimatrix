## ADR-005: Time-Boxed #193 Investigation with Store-Layer Scope Boundary

### Context

Issue #193 reports that `FeatureKnowledgeReuse` returns all zeros despite real MCP tool usage. The root cause is uncertain (SR-03). Potential failure points in the data flow:

1. **Session records have no query_log/injection_log rows.** If the sessions attributed to the feature had no `context_search` calls (because agents used briefing injection instead), query_log would legitimately be empty.
2. **Session ID format mismatch.** If session IDs in the `sessions` table differ from those in `query_log` (e.g., prefix differences), the `scan_query_log_by_sessions` query would return no results.
3. **Feature cycle attribution gap.** If sessions are attributed to `col-020` but query_log rows reference a different session_id format, the join fails silently.
4. **Store SQL bug.** A bug in `scan_query_log_by_sessions` or `scan_injection_log_by_sessions` could cause empty results.

The col-020b semantic revision (changing primary count from cross-session to all-delivery) will independently fix the "2+ sessions filter is too restrictive" part of #193. But if the underlying data flow returns empty slices, the revised semantics will still produce zeros.

### Decision

1. **Add debug tracing** at data flow boundaries in `compute_knowledge_reuse_for_sessions` (C6 in the architecture). This makes the root cause diagnosable without code changes.

2. **Time-box the investigation** during implementation. The implementer should:
   - Add the tracing
   - Run `context_retrospective` on a feature with known MCP tool usage
   - Check the debug logs for query_log and injection_log record counts
   - If counts are non-zero, the data flow is working and the bug was purely the 2+ sessions filter (fixed by the semantic revision)
   - If counts are zero, investigate session_id matching between tables

3. **Scope boundary:** If the root cause is in Store-layer SQL (option 4 above) or requires schema changes, file a separate issue. Do not expand col-020b into the Store crate. The field renames, normalization fix, and semantic revision are independently valuable and should ship regardless.

4. **The semantic revision is the primary fix.** Even if the data flow bug is confirmed, changing from "2+ sessions" to "all delivery" addresses the fundamental complaint: entries delivered to agents should count as knowledge delivery regardless of session count.

### Consequences

- **Easier:** col-020b ships with a clear scope boundary. The normalization fix (#192) and semantic revision (#193-partial) are not blocked by Store-layer investigation.
- **Easier:** Debug tracing provides a diagnostic tool for the human or a follow-up bugfix session.
- **Harder:** If the data flow bug is real and in the Store layer, a second fix is needed. The debug tracing makes that follow-up straightforward.
- **Risk:** If the human expects #193 to be fully resolved by col-020b, the scope boundary may need explicit communication. The architecture makes clear that the semantic revision is the primary fix, and data flow debugging is best-effort with a scope boundary.
