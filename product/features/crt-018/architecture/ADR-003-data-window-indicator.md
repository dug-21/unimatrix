## ADR-003: Data Window Indicator for Ephemeral Classifications

### Context

SR-02 identifies that session GC (30-day `DELETE_THRESHOLD_SECS`) deletes injection_log rows along with sessions, creating a sliding data window. Effectiveness classifications are non-deterministic across calls — the same entry may flip between Settled and Unmatched as sessions age out. Consumers of the effectiveness data need to understand the coverage of the analysis.

Two approaches:
- A) Document in code comments that classifications are ephemeral; no runtime indicator
- B) Include a `DataWindow` struct in the output showing session count, earliest/latest session timestamps

### Decision

**Option B: Include `DataWindow` in every effectiveness report.**

The `DataWindow` struct contains:
- `session_count: u32` — total sessions with outcomes in the retained window
- `earliest_session_at: Option<u64>` — Unix timestamp of oldest session (None if no sessions)
- `latest_session_at: Option<u64>` — Unix timestamp of newest session (None if no sessions)

Computed by a simple `SELECT COUNT(*), MIN(started_at), MAX(started_at) FROM sessions WHERE outcome IS NOT NULL` — negligible cost.

Displayed in output:
- Summary: appended to effectiveness line as `(N sessions over M days)`
- Markdown: shown as a note above the effectiveness tables
- JSON: included as `data_window` object in the `effectiveness` section

### Consequences

- Consumers can assess whether the analysis covers enough data to be meaningful (e.g., 3 sessions over 2 days vs 150 sessions over 28 days)
- Cost is one additional scalar SQL query (already inside the consolidated method)
- When session count is 0, effectiveness report has all entries as Unmatched/Settled with the data window showing no coverage — this is correct and self-documenting
- If GC retention changes (e.g., from 30 to 60 days), the data window automatically reflects the broader coverage without code changes
