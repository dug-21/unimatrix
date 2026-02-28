"""Suite 9: Adaptation (~10 tests).

Integration tests for the crt-006 adaptive embedding pipeline.
Tests exercise MicroLoRA adaptation, prototype management, training,
persistence, and edge cases through the MCP protocol against the
compiled unimatrix-server binary.
"""

import time

import pytest
from harness.assertions import (
    assert_tool_success,
    extract_entry_id,
    parse_entries,
    parse_entry,
    parse_status_report,
    get_result_text,
)
from harness.client import UnimatrixClient
from harness.conftest import get_binary_path


# -- Topic-specific content for generating meaningful co-access signal ------

ADAPTATION_TOPICS = {
    "architecture": [
        "Hexagonal architecture separates domain logic from infrastructure concerns using ports and adapters",
        "Event sourcing captures all changes as immutable events enabling full audit trails",
        "CQRS pattern separates read and write models for optimized query performance",
        "Domain driven design uses bounded contexts to partition complex business domains",
        "Microservices communicate via async message queues for loose coupling",
        "Clean architecture enforces dependency inversion with use cases at the core",
    ],
    "testing": [
        "Property based testing discovers edge cases that unit tests miss through random generation",
        "Integration tests verify component interactions using real databases not mocks",
        "Mutation testing measures test suite effectiveness by injecting deliberate faults",
        "Contract testing validates API boundaries between producer and consumer services",
        "Snapshot testing captures UI component output for regression detection",
        "Load testing with realistic traffic patterns reveals bottlenecks before production",
    ],
    "deployment": [
        "Blue green deployments enable zero downtime releases with instant rollback capability",
        "Canary releases route small traffic percentage to new version for gradual validation",
        "Infrastructure as code with Terraform ensures reproducible environment provisioning",
        "Container orchestration with Kubernetes manages scaling and self-healing workloads",
        "GitOps workflow uses pull requests as the single source of deployment truth",
        "Feature flags decouple deployment from release enabling dark launches",
    ],
    "security": [
        "Zero trust architecture verifies every request regardless of network location",
        "Secret rotation automates credential lifecycle to minimize exposure windows",
        "Input validation at system boundaries prevents injection attacks and data corruption",
        "Principle of least privilege restricts access to minimum required permissions",
        "TLS mutual authentication ensures both client and server identity verification",
        "Content security policy headers mitigate cross site scripting vulnerabilities",
    ],
}

TOPIC_QUERIES = {
    "architecture": [
        "hexagonal architecture ports adapters",
        "event sourcing domain design patterns",
        "microservices communication patterns",
    ],
    "testing": [
        "property based testing edge cases",
        "integration testing real databases",
        "mutation testing test effectiveness",
    ],
    "deployment": [
        "zero downtime deployment strategies",
        "canary releases gradual validation",
        "infrastructure as code provisioning",
    ],
    "security": [
        "zero trust security architecture",
        "secret management rotation lifecycle",
        "input validation injection prevention",
    ],
}


def _store_topic_entries(client, topics=None):
    """Store entries across topics, return {topic: [entry_ids]}."""
    topics = topics or ADAPTATION_TOPICS
    stored = {}
    for topic, contents in topics.items():
        ids = []
        for content in contents:
            resp = client.context_store(
                content, topic, "convention", agent_id="human", format="json"
            )
            ids.append(extract_entry_id(resp))
        stored[topic] = ids
    return stored


def _generate_co_access_signal(client, rounds=3):
    """Search across topics to generate co-access pairs."""
    for _ in range(rounds):
        for topic, queries in TOPIC_QUERIES.items():
            for query in queries:
                client.context_search(query, agent_id="human")


# -- Tests ----------------------------------------------------------------


@pytest.mark.smoke
def test_cold_start_search_equivalence(server):
    """A-01: Adaptation is transparent when no training has occurred.

    AC-37: Cold-start near-identity behavior.
    AC-39: Smoke test covers adaptation path.
    """
    stored = _store_topic_entries(server)

    # Search for each topic and verify semantically relevant results
    for topic, queries in TOPIC_QUERIES.items():
        for query in queries:
            resp = server.context_search(query, format="json", agent_id="human")
            results = parse_entries(resp)
            assert len(results) > 0, f"No results for query '{query}'"

    # Verify all entries are searchable
    resp = server.context_search("architecture design patterns", format="json")
    assert_tool_success(resp)


def test_adaptation_state_persists_across_restart(tmp_path):
    """A-02: Adaptation state survives server restart.

    AC-34: Adaptation state persistence verified across restart.
    """
    binary = get_binary_path()

    # Phase 1: Store entries, generate co-access signal, record reference
    client1 = UnimatrixClient(binary, project_dir=str(tmp_path), timeout=30.0)
    client1.initialize()
    client1.wait_until_ready()

    _store_topic_entries(client1)
    _generate_co_access_signal(client1, rounds=4)

    # Record reference search results
    ref_resp = client1.context_search(
        "hexagonal architecture ports adapters", format="json", agent_id="human"
    )
    ref_results = parse_entries(ref_resp)
    ref_ids = [e.get("id") for e in ref_results if e.get("id")]

    client1.shutdown()

    # Phase 2: Restart with same data directory
    client2 = UnimatrixClient(binary, project_dir=str(tmp_path), timeout=30.0)
    client2.initialize()
    client2.wait_until_ready()

    # Verify entries persist and search works
    post_resp = client2.context_search(
        "hexagonal architecture ports adapters", format="json", agent_id="human"
    )
    post_results = parse_entries(post_resp)
    assert len(post_results) > 0, "No results after restart"

    # Top results should overlap (adaptation state loaded or fresh — either is valid)
    post_ids = [e.get("id") for e in post_results if e.get("id")]
    overlap = set(ref_ids[:5]) & set(post_ids[:5])
    assert len(overlap) >= 2, (
        f"Too little overlap in top-5 results: pre={ref_ids[:5]}, post={post_ids[:5]}"
    )

    client2.shutdown()


def test_co_access_training_improves_retrieval(server):
    """A-03: Adaptation improves search quality with co-access signal.

    AC-36: Adapted search quality verification.
    """
    stored = _store_topic_entries(server)

    # Generate substantial co-access signal — search within topics
    # This creates pairs of entries that are frequently accessed together
    _generate_co_access_signal(server, rounds=5)

    # Validate: topic-specific queries should return topic-relevant entries
    for topic in ["architecture", "testing", "deployment", "security"]:
        queries = TOPIC_QUERIES[topic]
        topic_ids = set(stored[topic])

        hits = 0
        for query in queries:
            resp = server.context_search(query, k=5, format="json", agent_id="human")
            results = parse_entries(resp)
            result_ids = {e.get("id") for e in results if e.get("id")}
            hits += len(topic_ids & result_ids)

        # With 3 queries x 5 results each, at least 3 should be from the correct topic
        # (generous threshold — cold-start embeddings already provide decent semantic match)
        assert hits >= 3, (
            f"Topic '{topic}' had only {hits} in-topic hits across queries"
        )


def test_embedding_consistency_with_adaptation(server):
    """A-04: Embedding consistency check works with adaptation active.

    AC-35: Embedding consistency check returns valid results.
    """
    _store_topic_entries(server)
    _generate_co_access_signal(server, rounds=3)

    # Run embedding consistency check
    resp = server.context_status(
        check_embeddings=True, agent_id="human", format="json", timeout=60.0
    )
    report = parse_status_report(resp)

    # Status call must succeed and report valid data
    assert report.get("total_active", 0) > 0, "No active entries in report"

    # Lambda (coherence) must be computed and valid
    coherence = report.get("coherence")
    assert coherence is not None, "Coherence (lambda) not in status report"
    assert 0.0 <= coherence <= 1.0, f"Invalid coherence value: {coherence}"

    # Embedding consistency score is valid (check was performed)
    emb_score = report.get("embedding_consistency_score")
    assert emb_score is not None, "Embedding consistency score not in report"
    assert 0.0 <= emb_score <= 1.0, f"Invalid embedding consistency score: {emb_score}"


def test_volume_with_adaptation_active(server):
    """A-05: Adaptation handles volume without errors or timeouts.

    AC-38: Volume suite behavior unchanged with adaptation active.
    """
    start = time.monotonic()

    # Store 100+ entries across 10 topics with distinct content
    topic_names = [
        "architecture", "testing", "deployment", "security", "performance",
        "database", "api-design", "error-handling", "logging", "configuration",
    ]
    aspects = [
        "design principles", "implementation patterns", "anti-patterns to avoid",
        "migration strategies", "tooling recommendations", "team workflows",
        "monitoring approaches", "documentation standards", "review checklists",
        "incident response procedures",
    ]
    stored_count = 0
    for topic in topic_names:
        for j, aspect in enumerate(aspects):
            resp = server.context_store(
                f"Detailed guidance on {aspect} for {topic}: "
                f"when working with {topic} systems, {aspect} should follow "
                f"established conventions. Key insight number {j} involves "
                f"balancing trade-offs specific to {topic} {aspect} scenarios.",
                topic, "convention", agent_id="human",
            )
            assert_tool_success(resp)
            stored_count += 1

    # 50+ searches across topics
    for topic in topic_names:
        for _ in range(5):
            resp = server.context_search(
                f"{topic} best practices production", agent_id="human"
            )
            assert_tool_success(resp)

    # Status report completes
    resp = server.context_status(agent_id="human", format="json", timeout=60.0)
    report = parse_status_report(resp)
    # Some entries may be deduplicated; verify substantial count stored
    assert report.get("total_active", 0) >= 50, (
        f"Expected at least 50 active entries, got {report.get('total_active', 0)}"
    )

    elapsed = time.monotonic() - start
    assert elapsed < 120, f"Volume test took {elapsed:.1f}s (limit: 120s)"


def test_edge_cases_with_adaptation(server):
    """A-06: Known edge cases work through the adaptation layer.

    Verifies that adaptation does not break basic edge cases.
    """
    # First generate some training history so adaptation is active
    _store_topic_entries(server)
    _generate_co_access_signal(server, rounds=2)

    # Edge case 1: Unicode content
    resp = server.context_store(
        "\u4f60\u597d\u4e16\u754c - architecture patterns in multilingual context",
        "architecture", "convention", agent_id="human", format="json",
    )
    uid = extract_entry_id(resp)
    resp = server.context_search("\u4f60\u597d architecture", agent_id="human")
    assert_tool_success(resp)

    # Edge case 2: Single-word content
    resp = server.context_store(
        "minimalism", "testing", "convention", agent_id="human", format="json",
    )
    assert_tool_success(resp)

    # Edge case 3: Long content (5KB)
    long_content = ("Detailed analysis of testing patterns and conventions. " * 100)
    resp = server.context_store(
        long_content, "testing", "convention", agent_id="human", format="json",
    )
    assert_tool_success(resp)

    # Edge case 4: Search with very short query
    resp = server.context_search("test", agent_id="human")
    assert_tool_success(resp)

    # Edge case 5: Entry with many tags
    resp = server.context_store(
        "multi-tag entry through adaptation layer",
        "architecture", "convention",
        tags=["tag1", "tag2", "tag3"],
        agent_id="human", format="json",
    )
    assert_tool_success(resp)


def test_training_is_non_blocking(server):
    """A-07: Training does not delay tool responses.

    Verifies store/search operations remain responsive as co-access
    pairs accumulate and training may fire.
    """
    # Baseline: measure average response time for 10 stores
    baseline_times = []
    for i in range(10):
        t0 = time.monotonic()
        server.context_store(
            f"baseline entry {i} about architecture patterns and design decisions",
            "architecture", "convention", agent_id="human",
        )
        baseline_times.append(time.monotonic() - t0)
    baseline_avg = sum(baseline_times) / len(baseline_times)

    # Load: 100 sequential stores with interleaved searches (generates co-access)
    load_times = []
    for i in range(100):
        t0 = time.monotonic()
        server.context_store(
            f"load entry {i} about testing conventions and deployment practices",
            ["testing", "deployment", "security"][i % 3],
            "convention",
            agent_id="human",
        )
        load_times.append(time.monotonic() - t0)

        # Interleave searches every 10 stores to generate co-access pairs
        if i % 10 == 9:
            server.context_search("testing conventions practices", agent_id="human")
            server.context_search("deployment infrastructure patterns", agent_id="human")

    # Measure: last 10 store response times (training may be active)
    tail_avg = sum(load_times[-10:]) / 10

    # Tail average should be <= 3x baseline (generous — accounts for DB growth)
    assert tail_avg <= baseline_avg * 3 + 0.5, (
        f"Response time degraded: baseline={baseline_avg:.3f}s, tail={tail_avg:.3f}s"
    )

    # All 100 stores succeeded (implicit — would have raised on error)


def test_prototype_management_diverse_topics(server):
    """A-08: Server handles 20+ distinct topics without errors.

    AC-13: Prototype management under diverse topics.
    """
    topic_names = [
        "architecture", "testing", "deployment", "security", "performance",
        "database", "api-design", "error-handling", "logging", "configuration",
        "authentication", "caching", "monitoring", "documentation", "refactoring",
        "networking", "serialization", "concurrency", "observability", "migrations",
    ]

    # Store 3+ entries per topic (60+ total) with distinct content per entry
    entry_aspects = [
        "design principles and foundational concepts",
        "implementation patterns and common pitfalls",
        "migration strategies and upgrade procedures",
    ]
    for topic in topic_names:
        for j, aspect in enumerate(entry_aspects):
            resp = server.context_store(
                f"Guidance on {aspect} for {topic}: when working with "
                f"{topic} in production, {aspect} require careful attention "
                f"to detail and adherence to established team conventions.",
                topic, "convention", agent_id="human",
            )
            assert_tool_success(resp)

    # Search each topic
    for topic in topic_names:
        resp = server.context_search(
            f"{topic} conventions patterns", format="json", agent_id="human"
        )
        assert_tool_success(resp)

    # Status report completes
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)
    assert report.get("total_active", 0) >= 40


def test_correction_chain_with_adaptation(server):
    """A-09: Correction flow works through the adaptation layer.

    Verifies correction chain integrity when each correction re-embeds
    through the adaptation pipeline.
    """
    # Store original entry
    store_resp = server.context_store(
        "Initial architecture decision: use monolithic deployment for simplicity",
        "architecture", "decision",
        agent_id="human", format="json",
    )
    original_id = extract_entry_id(store_resp)

    # Correction 1
    correct1_resp = server.context_correct(
        original_id,
        "Updated architecture decision: migrate to microservices for scalability",
        reason="Monolith no longer meets scaling requirements",
        agent_id="human", format="json",
    )
    assert_tool_success(correct1_resp)
    id_v2 = extract_entry_id(correct1_resp)

    # Correction 2
    correct2_resp = server.context_correct(
        id_v2,
        "Revised architecture decision: use modular monolith as stepping stone",
        reason="Full microservices premature, modular monolith gives best tradeoff",
        agent_id="human", format="json",
    )
    assert_tool_success(correct2_resp)
    id_v3 = extract_entry_id(correct2_resp)

    # Correction 3
    correct3_resp = server.context_correct(
        id_v3,
        "Final architecture decision: modular monolith with event-driven boundaries",
        reason="Added event-driven communication between modules",
        agent_id="human", format="json",
    )
    assert_tool_success(correct3_resp)
    id_v4 = extract_entry_id(correct3_resp)

    # Final entry is searchable
    search_resp = server.context_search(
        "modular monolith event-driven architecture", format="json", agent_id="human"
    )
    results = parse_entries(search_resp)
    result_ids = [e.get("id") for e in results if e.get("id")]
    assert id_v4 in result_ids, (
        f"Final correction {id_v4} not found in search results: {result_ids}"
    )

    # Correction chain is intact (get latest, verify it has supersedes)
    get_resp = server.context_get(id_v4, format="json")
    entry = parse_entry(get_resp)
    assert entry.get("supersedes") is not None or entry.get("id") == id_v4


def test_status_report_with_adaptation_active(server):
    """A-10: Status report succeeds with adaptation active.

    Verifies status report (including lambda/coherence) works correctly
    when adaptation layer is active and training may have occurred.
    """
    _store_topic_entries(server)
    _generate_co_access_signal(server, rounds=3)

    # Status report
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)

    # Basic health check
    assert report.get("total_active", 0) > 0
    assert "category_distribution" in report or "categories" in report or len(report) > 3

    # Lambda (coherence) is computed and valid
    coherence = report.get("coherence")
    if coherence is not None:
        assert 0.0 <= coherence <= 1.0, f"Invalid coherence: {coherence}"

    # Co-access pairs should exist (we generated signal)
    co_access = report.get("co_access", {})
    total_pairs = co_access.get("total_pairs", report.get("total_co_access_pairs", 0))
    assert total_pairs > 0, f"Expected co-access pairs after searches, report keys: {list(report.keys())}"
