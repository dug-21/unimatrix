## ADR-007: Feature Boost in Briefing via Score Adjustment

### Context

`context_briefing` accepts an optional `feature` parameter. When provided, entries tagged with the feature ID should be prioritized in the semantic search results. Two approaches were considered:

1. **Query modification**: Run a separate query filtered to the feature tag, then merge with the general search results.
2. **Score adjustment**: Run a single search query, then reorder the returned results to prioritize entries whose tags include the feature ID.

### Decision

Use score adjustment (option 2). After the semantic search returns results, iterate over the results and boost entries whose `tags` vector contains the feature string. The boost is a reordering: feature-tagged entries are moved to the front of the relevant context section, maintaining their relative similarity order within the boosted group.

No extra database queries or embedding operations are needed. The boost is applied to the already-returned results (typically k=3), making it O(k) -- negligible cost.

Entries without the feature tag are NOT filtered out. They remain in the results, just ranked after feature-tagged entries. This ensures the briefing still includes generally relevant context even when no feature-specific entries exist.

### Consequences

**Easier:**
- Single search query (no extra queries for feature filtering)
- Deterministic reordering: easy to test and reason about
- Gracefully handles the case where no entries have the feature tag (results unchanged)

**Harder:**
- Feature boost is binary (tagged or not) rather than a similarity score adjustment
- If more entries match the feature than k, some feature-relevant entries may not appear (mitigated: k=3 is small, and tag matching is imprecise anyway)
