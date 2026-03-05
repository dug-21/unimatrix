## ADR-003: Silent Event Loss When Server Unavailable is Accepted

### Context

With JSONL elimination, observation data flows exclusively through the UDS socket. If the server process is not running when a hook fires, the event is lost. The current JSONL path provided a durable fallback -- events were written to disk regardless of server state.

SR-06 flagged this as a scope boundary risk: removing JSONL removes the only offline observation mechanism.

### Decision

Accept silent event loss when the server is unavailable.

Rationale:
1. **Current behavior already accepts this**: Hook scripts exit 0 on all failures (FR-03.7). UDS connection failures are silently swallowed. Events are already lost when the server is down -- they just also happen to be written to JSONL (which nothing reads in real-time).
2. **Event queue provides retry**: The hook CLI already has an `EventQueue` that queues failed sends and replays them on the next successful connection. This covers transient failures (server restarting, brief unavailability).
3. **Observation data is telemetry, not critical state**: Lost observation events degrade retrospective analysis quality but do not break functionality. The system continues to operate. Detection rules produce results from whatever data is available.
4. **Server uptime is high**: The server starts on first hook and stays running for the session lifetime. Extended downtime is uncommon in practice.

### Consequences

- **No fallback buffer needed**: No new file-based buffering mechanism.
- **Event queue is sufficient**: Transient failures are handled by the existing replay mechanism.
- **Retrospective quality degrades gracefully**: Missing events produce incomplete (not incorrect) analysis.
- **Monitoring gap**: No way to know how many events were lost. If this becomes a concern, a counter in the event queue could track drop count. Deferred.
