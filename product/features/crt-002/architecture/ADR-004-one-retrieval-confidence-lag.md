## ADR-004: One-Retrieval Confidence Lag

### Context

When an agent retrieves entries via `context_search`, the response includes the entry's `confidence` value. Confidence is recomputed after the response is formatted (in the fire-and-forget usage recording path). This means the confidence shown in the response is from the PREVIOUS retrieval's computation, not the current one.

The alternative -- computing confidence before formatting the response -- would require either: (a) a write transaction before the read (breaking the read-first pattern), or (b) computing confidence without writing it, then writing it afterward (computing twice), or (c) blocking the response on the confidence write (breaking fire-and-forget).

### Decision

Accept the one-retrieval lag. The confidence displayed in a retrieval response reflects the state from the previous retrieval (or from insert/mutation). The current retrieval updates confidence in the background after the response is sent.

**Why this is acceptable:**
1. For most entries, the only component that changes between retrievals is freshness (time-dependent). Usage, helpfulness, corrections, and trust are updated at retrieval time -- but those updates apply to the CURRENT retrieval, which the agent already received.
2. The first retrieval of a never-accessed entry shows the initial confidence from insert time. The second retrieval shows confidence with the first retrieval's usage data. This is a single-observation lag.
3. The fire-and-forget pattern is a core architectural decision from crt-001 (ADR-004 in crt-001). Confidence must respect it.

### Consequences

**Easier:**
- Maintains the fire-and-forget pattern (no blocking writes in the response path)
- No double computation
- Response latency is unaffected by confidence computation

**Harder:**
- The first retrieval of a new entry shows confidence from insert time (which may be slightly stale if time has passed)
- Agents cannot rely on the displayed confidence being perfectly current -- but the lag is at most one retrieval, which is operationally insignificant
