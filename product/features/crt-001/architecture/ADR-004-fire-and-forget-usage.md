## ADR-004: Fire-and-Forget Usage Recording

### Context

Usage recording happens after every successful retrieval. If usage recording fails (e.g., write transaction error), should the tool call fail?

Option A: Propagate usage recording errors to the tool caller. The agent sees an error even though the retrieval succeeded.

Option B: Log usage recording errors internally but return the successful retrieval result to the agent. The tool call succeeds; usage data may be incomplete.

### Decision

Use Option B: fire-and-forget. Usage recording errors are logged (via `tracing::warn!`) but do not affect the tool result.

Rationale:
- **Usage is a side effect, not the purpose.** The agent called `context_search` to get search results, not to record usage data. Failing the search because of a usage write error is a poor user experience.
- **Consistent with AUDIT_LOG pattern.** If audit logging fails in the current tools.rs, the tool result is still returned (the audit write is the last step and errors are logged, not propagated).
- **Usage data is tolerant of gaps.** Missing a few usage events doesn't affect crt-002's confidence formula significantly. The data is statistical -- occasional gaps are noise.
- **Debugging via tracing.** Usage recording failures are logged with entry IDs and error details. Operations teams can investigate if failures become frequent.

### Consequences

- **Tool callers never see usage recording errors.** The response is always the retrieval result, even if usage recording failed entirely.
- **Usage data may have occasional gaps.** This is acceptable for statistical signals like access_count and helpful_count.
- **Logging overhead.** Each usage recording failure generates a warning log entry. If the store is consistently failing writes (e.g., disk full), the logs will be noisy. This is a feature, not a bug -- it surfaces the operational problem.
- **No retry mechanism.** Failed usage recordings are not retried. If the write transaction fails, those usage events are lost. Retry adds complexity for marginal benefit.
