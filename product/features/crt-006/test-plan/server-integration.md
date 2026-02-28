# Test Plan: server-integration

## Component Under Test

Server-side integration of AdaptationService into UnimatrixServer -- write path, read path, training path, startup/shutdown, coherence check. Tested via the Python integration test suite at `product/test/infra-001/suites/test_adaptation.py`.

## Risks Covered

- **R-10** (High): Embedding consistency check false positives
- **IR-01**: Write path dimension/normalization errors
- **IR-02**: Query/entry space mismatch
- **IR-03**: Co-access pair recording feeds training reservoir
- **IR-04**: Shutdown persistence order
- **IR-05**: Maintenance re-indexing with current weights

## Integration Test Cases

All tests run against the compiled `unimatrix-server` binary via MCP JSON-RPC over stdio.

### A-01: Cold-start search equivalence (AC-37, AC-39)

**Purpose**: Verify adaptation layer is transparent when no training has occurred.
**Fixture**: Fresh server (no co-access history).
**Method**:
- Store 10 entries across 3 topics with distinct content
- Search for topic-specific queries
**Assertions**:
- Search returns semantically relevant entries (adaptation is near-identity)
- No errors in any tool call
- Response times within normal bounds
**Marks**: `@pytest.mark.smoke` (minimum gate requirement)
**Risk Coverage**: R-12, IR-01, IR-02

### A-02: Adaptation state persists across restart (AC-34)

**Purpose**: Verify adaptation state survives server restart.
**Fixture**: Server with co-access training signal.
**Method**:
- Store 20 entries across 4 topics
- Perform 30 topic-specific searches to generate co-access pairs
- Wait for training to potentially trigger (fire-and-forget timing)
- Record search results for a reference query
- Shutdown server
- Restart server with same data directory
- Search for same reference query
**Assertions**:
- Server starts successfully (adaptation state loaded or fresh)
- Search results after restart are consistent with pre-restart results
- If adaptation state file exists in data dir, it was loaded
**Risk Coverage**: R-04, IR-04

### A-03: Co-access training improves retrieval (AC-36)

**Purpose**: Validate the core value proposition -- adaptation improves search quality.
**Fixture**: Fresh server.
**Method**:
- Store 30 entries across 4 topics (architecture decisions, coding patterns, testing conventions, deployment procedures)
- Perform 40 searches -- 10 per topic, using topic-specific queries
- Each search generates co-access pairs within the topic
- After enough searches for training to trigger, perform final validation searches
**Assertions**:
- For each topic: the top-3 search results contain at least 2 entries from the correct topic
- Search results are at least as good as cold-start (no regression)
- Training generation counter > 0 (if accessible via status)
**Risk Coverage**: R-03, IR-01, IR-02, IR-03
**Note**: This test may need generous thresholds -- adaptation improvement depends on co-access signal quality and training step count.

### A-04: Embedding consistency check with adaptation (AC-35)

**Purpose**: Verify crt-005 coherence gate works with adaptation active.
**Fixture**: Server with training signal (post-training).
**Method**:
- Store 15 entries
- Generate co-access pairs via searches
- Call `context_status` with `check_embeddings=True`
**Assertions**:
- Status call succeeds (no error)
- Embedding consistency check completes
- If adaptation is active, the consistency check uses adapted re-embeddings
- Lambda metric is computed (no NaN in any dimension)
**Risk Coverage**: R-10, IR-05

### A-05: Volume behavior with adaptation active (AC-38)

**Purpose**: Verify adaptation handles volume without errors or timeouts.
**Fixture**: Fresh server.
**Method**:
- Store 100+ entries with diverse topics (at least 10 topics)
- Perform 50+ searches across topics
- Call `context_status` with and without `maintain`
**Assertions**:
- All tool calls succeed
- Status report completes
- Total test time < 120 seconds (volume suite timeout)
- No errors in server logs
**Risk Coverage**: IR-01, IR-02, IR-03

### A-06: Edge cases with adaptation active (AC-31)

**Purpose**: Verify known edge cases still work through the adaptation layer.
**Fixture**: Server with some training history.
**Method**: Execute the following edge cases:
- Unicode content: store entry with Unicode title and content, search for it
- Single-entry DB: store one entry, search for it
- Empty search: search with empty string (if supported)
- Minimal content: store entry with single-word content, search for it
- Long content: store entry with 10KB content, search for it
**Assertions**:
- No errors in any tool call
- Search returns the stored entry (when applicable)
- Adaptation does not crash on any edge case

### A-07: Training is non-blocking (AC-22, AC-23)

**Purpose**: Verify training does not delay tool responses.
**Fixture**: Fresh server.
**Method**:
- Baseline: measure average response time for 10 store operations
- Load: perform 100 sequential store operations with co-access pair generation
- Measure: average response time for last 10 store operations (training may be firing)
**Assertions**:
- Average response time under load <= 2x baseline response time
- No tool call times out
- All 100 store operations succeed

### A-08: Prototype management under diverse topics (AC-13)

**Purpose**: Verify server handles many distinct topics without errors.
**Fixture**: Fresh server.
**Method**:
- Store entries across 20+ distinct topics, each with 3+ entries (60+ total entries)
- Search for entries in each topic
**Assertions**:
- No errors in any operation
- Search returns relevant results for each topic
- Status report completes successfully

### A-09: Correction chain with adaptation (AC-31)

**Purpose**: Verify correction flow works through adaptation layer.
**Fixture**: Fresh server.
**Method**:
- Store an entry
- Correct it 3 times (creating a 4-entry correction chain)
- Search for the final corrected content
**Assertions**:
- All corrections succeed
- Final entry is searchable
- Correction chain links are intact (get the latest entry, verify it has chain)

### A-10: Status report with adaptation metadata (AC-21)

**Purpose**: Verify adaptation information is accessible via status.
**Fixture**: Server with training history.
**Method**:
- Generate co-access signal via searches
- Call `context_status`
**Assertions**:
- Status call succeeds
- If adaptation metadata is exposed (training generation), it has a valid value
- Lambda metric is computed (existing crt-005 functionality not broken)

## Fixture Design

### `trained_server` fixture (function-scoped)

```python
@pytest.fixture
def trained_server(tmp_path):
    """Server with co-access training signal available."""
    server = start_server(tmp_path)
    client = UnimatrixClient(server)

    # Store 30 entries across 5 topics
    topics = ["architecture", "testing", "deployment", "patterns", "decisions"]
    for topic in topics:
        for i in range(6):
            client.context_store(
                content=f"Content about {topic} item {i}...",
                topic=topic,
                category="convention",
            )

    # Generate co-access signal via 25 searches
    for topic in topics:
        for _ in range(5):
            client.context_search(query=f"{topic} best practices")

    yield client

    server.shutdown()
```

### Smoke test selection

A-01 is marked `@pytest.mark.smoke` as the minimum gate requirement for crt-006. It validates the most basic invariant: adaptation does not break normal search.

## Edge Case Coverage

| Edge Case | Integration Test |
|-----------|-----------------|
| EC-01 Empty KB | A-01 (implicit -- starts empty) |
| EC-02 Single entry | A-06 |
| EC-05 Corrected entry | A-09 |
| EC-06 Unicode content | A-06 |

## Total: 10 integration tests
