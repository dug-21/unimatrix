# infra-001: Dockerized Integration Test Harness

## Problem

Unimatrix has 745+ unit tests that validate individual components in isolation. What's missing is a **system-level test bed** that exercises the compiled binary through the MCP protocol — the actual interface agents use — with high volume, lifecycle scenarios, security probes, and edge cases. Without this, we can trust individual functions but not the assembled system.

## Goals

1. **Dockerized**: `docker compose up --abort-on-container-exit` runs everything, `docker compose down` tears it down. Zero host dependencies beyond Docker.
2. **Multi-suite**: Independent test suites that can run individually or all together.
3. **High volume**: Stress tests with thousands of entries, concurrent operations, large payloads.
4. **Trust-building**: Cover the full threat model — security, lifecycle integrity, confidence math, contradiction detection.
5. **CI-ready**: Exit code 0 = pass, non-zero = fail. JUnit XML output for CI integration.
6. **Fast feedback**: Suites are parallelizable. Individual suites run in under 60 seconds (except stress).

## Non-Goals

- Replacing existing `cargo test` unit tests (they stay where they are).
- Performance benchmarking with precise latency targets (that's a separate effort).
- UI/dashboard testing (no UI yet).
- Multi-project isolation testing (M7 feature, not yet built).

---

## Architecture

```
product/test/infra-001/
├── DESIGN.md                      # This document
├── Dockerfile                     # Multi-stage: build binary + test runtime
├── docker-compose.yml             # Orchestration: server build + test runner
├── harness/                       # Python MCP test client library
│   ├── __init__.py
│   ├── client.py                  # MCP JSON-RPC client over stdio subprocess
│   ├── generators.py              # Test data factories (entries, agents, scenarios)
│   ├── assertions.py              # Custom assertion helpers
│   └── conftest.py                # pytest fixtures (server lifecycle, fresh DB)
├── suites/                        # Test suites (pytest modules)
│   ├── test_protocol.py           # Suite 1: MCP protocol & handshake
│   ├── test_tools.py              # Suite 2: All 9 tools happy + error paths
│   ├── test_lifecycle.py          # Suite 3: Knowledge lifecycle flows
│   ├── test_volume.py             # Suite 4: Scale & stress
│   ├── test_security.py           # Suite 5: Security & hardening
│   ├── test_confidence.py         # Suite 6: Confidence system validation
│   ├── test_contradiction.py      # Suite 7: Contradiction detection
│   ├── test_edge_cases.py         # Suite 8: Edge cases & boundary conditions
│   └── conftest.py                # Suite-level fixtures
├── fixtures/                      # Static test data
│   ├── injection_patterns.json    # Known prompt injection payloads
│   ├── pii_samples.json           # PII detection test cases
│   ├── unicode_corpus.json        # Unicode edge cases (CJK, RTL, emoji, ZWJ)
│   └── large_entries.json         # Near-max-size entry payloads
├── scripts/
│   ├── run.sh                     # Entrypoint: run all or selected suites
│   └── report.sh                  # Collect and format results
└── pytest.ini                     # pytest configuration
```

---

## Docker Strategy

### Multi-Stage Dockerfile

```
Stage 1 — builder:
  FROM rust:1.89-bookworm
  - Copy workspace, build release binary
  - Run cargo test --lib (unit tests in Docker = baseline)
  - Output: /app/target/release/unimatrix-server

Stage 2 — test-runtime:
  FROM python:3.12-slim-bookworm
  - Install ONNX Runtime shared lib (matches ort 2.0.0-rc.9)
  - Copy binary from builder
  - Copy harness/, suites/, fixtures/
  - pip install pytest pytest-xdist pytest-timeout pytest-json-report
  - ENTRYPOINT: run.sh
```

### docker-compose.yml

Two services, one build:

```yaml
services:
  # Build stage — compiles binary + runs unit tests
  builder:
    build:
      context: ../..       # workspace root
      dockerfile: product/test/infra-001/Dockerfile
      target: builder
    # No runtime — build-only

  # Test runner — exercises binary via MCP protocol
  test-runner:
    build:
      context: ../..
      dockerfile: product/test/infra-001/Dockerfile
      target: test-runtime
    environment:
      - UNIMATRIX_BINARY=/app/unimatrix-server
      - RUST_LOG=warn
      - TEST_SUITE=${TEST_SUITE:-all}    # Override to run specific suite
      - TEST_WORKERS=${TEST_WORKERS:-4}  # Parallel workers
    volumes:
      - test-results:/results
    tmpfs:
      - /tmp:size=512M                   # Temp DBs live here

volumes:
  test-results:
```

### Usage

```bash
# Run everything
docker compose -f product/test/infra-001/docker-compose.yml up --build --abort-on-container-exit

# Run specific suite
TEST_SUITE=security docker compose -f product/test/infra-001/docker-compose.yml up --build --abort-on-container-exit

# Run with more parallelism
TEST_WORKERS=8 docker compose up --build --abort-on-container-exit

# Teardown
docker compose -f product/test/infra-001/docker-compose.yml down -v
```

---

## MCP Client Library (`harness/client.py`)

The core abstraction: a Python class that manages a `unimatrix-server` subprocess and speaks MCP JSON-RPC over stdin/stdout.

### Key Design Decisions

1. **One server per test function** (default). Fresh database, no state leakage. pytest fixture handles lifecycle.
2. **Shared server option** for lifecycle/volume suites where accumulating state IS the test.
3. **Timeout enforcement**: every MCP call times out after 10s (configurable). Catches hangs.
4. **Structured responses**: parse JSON-RPC responses into typed Python dataclasses for assertion ergonomics.

### Interface

```python
class UnimatrixClient:
    """MCP client that manages a unimatrix-server subprocess."""

    def __init__(self, binary_path: str, project_dir: str | None = None, verbose: bool = False):
        """Spawn server, complete MCP initialize handshake."""

    # --- MCP lifecycle ---
    def initialize(self) -> dict:
        """Send initialize request, return server capabilities."""

    def shutdown(self):
        """Send shutdown, wait for clean exit."""

    # --- Tools (typed wrappers around call_tool) ---
    def context_store(self, content: str, topic: str, category: str, **kwargs) -> dict:
        ...

    def context_search(self, query: str, **kwargs) -> dict:
        ...

    def context_lookup(self, **kwargs) -> dict:
        ...

    def context_get(self, id: int) -> dict:
        ...

    def context_correct(self, original_id: int, content: str, **kwargs) -> dict:
        ...

    def context_deprecate(self, id: int, **kwargs) -> dict:
        ...

    def context_status(self, **kwargs) -> dict:
        ...

    def context_briefing(self, role: str, task: str, **kwargs) -> dict:
        ...

    def context_quarantine(self, id: int, action: str = "quarantine", **kwargs) -> dict:
        ...

    # --- Low-level ---
    def call_tool(self, name: str, arguments: dict, timeout: float = 10.0) -> dict:
        """Send tools/call JSON-RPC request, return parsed result."""

    def send_raw(self, method: str, params: dict) -> dict:
        """Send arbitrary JSON-RPC request."""
```

### pytest Fixtures

```python
@pytest.fixture
def server(tmp_path):
    """Fresh server per test. Yields UnimatrixClient, shuts down on exit."""
    client = UnimatrixClient(BINARY_PATH, project_dir=str(tmp_path))
    client.initialize()
    yield client
    client.shutdown()

@pytest.fixture(scope="module")
def shared_server(tmp_path_factory):
    """Shared server for lifecycle/volume suites. One per module."""
    ...

@pytest.fixture
def populated_server(server):
    """Server pre-loaded with a standard dataset (50 entries, 5 topics, 3 agents)."""
    ...
```

---

## Test Data Generation (`harness/generators.py`)

Factories that produce realistic, varied test data:

```python
def make_entry(*, topic=None, category=None, content=None, tags=None, agent_id=None) -> dict:
    """Single entry with realistic defaults and optional overrides."""

def make_entries(n: int, *, topic_distribution=None, category_mix=None) -> list[dict]:
    """Batch of n entries with controlled distribution across topics/categories."""

def make_contradicting_pair(topic: str) -> tuple[dict, dict]:
    """Two entries with high semantic similarity but conflicting directives."""

def make_correction_chain(depth: int) -> list[dict]:
    """A chain of entries where each corrects the previous."""

def make_injection_payloads() -> list[str]:
    """Content strings designed to test prompt injection defenses."""

def make_pii_content() -> list[tuple[str, str]]:
    """(content, expected_pii_type) pairs for scanner validation."""

def make_unicode_edge_cases() -> list[str]:
    """Content with CJK, RTL, emoji, ZWJ sequences, combining chars, etc."""

def make_bulk_dataset(n: int) -> list[dict]:
    """Large dataset for volume testing. Distinct embeddings, varied metadata."""
```

---

## Test Suites

### Suite 1: MCP Protocol (`test_protocol.py`) — ~15 tests

Validates the MCP handshake and protocol compliance.

| Test | What it validates |
|------|-------------------|
| `test_initialize_returns_capabilities` | Server responds to `initialize` with valid capabilities |
| `test_server_info` | Server name, version present |
| `test_list_tools_returns_all_nine` | All 9 `context_*` tools listed with schemas |
| `test_tool_schemas_valid_jsonschema` | Each tool's inputSchema is valid JSON Schema |
| `test_unknown_tool_returns_error` | Calling nonexistent tool returns proper error |
| `test_malformed_jsonrpc_rejected` | Invalid JSON-RPC envelope rejected gracefully |
| `test_missing_params_rejected` | Required params missing → clear error |
| `test_concurrent_requests` | Multiple requests in flight don't corrupt state |
| `test_notifications_ignored` | Server ignores notifications (per MCP spec) |
| `test_graceful_shutdown` | Shutdown request → clean exit code 0 |
| `test_server_survives_invalid_utf8` | Binary input on stdin doesn't crash server |
| `test_large_request_payload` | Very large JSON-RPC request handled (or rejected cleanly) |
| `test_empty_tool_arguments` | Empty `{}` arguments handled per tool |
| `test_extra_unknown_fields_ignored` | Unknown fields in arguments don't cause errors |
| `test_response_format_json_parseable` | All tool responses are valid JSON when format=json |

### Suite 2: Tool Coverage (`test_tools.py`) — ~80 tests

Every tool, every parameter, every error condition.

#### context_store (~15 tests)
| Test | What it validates |
|------|-------------------|
| `test_store_minimal` | Store with just content, topic, category succeeds |
| `test_store_all_fields` | Store with title, tags, source, agent_id, feature succeeds |
| `test_store_returns_entry_id` | Response contains the new entry's ID |
| `test_store_roundtrip` | Store then get returns identical content |
| `test_store_near_duplicate_detected` | Storing very similar content warns about duplicate |
| `test_store_content_hash_populated` | Stored entry has SHA-256 content_hash |
| `test_store_created_by_set` | agent_id propagates to created_by field |
| `test_store_timestamps_set` | created_at and updated_at are populated |
| `test_store_invalid_category_rejected` | Category not in allowlist → error |
| `test_store_empty_content_rejected` | Empty content string → validation error |
| `test_store_oversized_content_rejected` | Content >1M chars → validation error |
| `test_store_empty_topic_rejected` | Empty topic → validation error |
| `test_store_special_chars_in_topic_rejected` | Control chars in topic → validation error |
| `test_store_many_tags` | Storing with maximum tags succeeds |
| `test_store_duplicate_tags_deduplicated` | Duplicate tags in request handled |

#### context_search (~12 tests)
| Test | What it validates |
|------|-------------------|
| `test_search_finds_relevant` | Semantic search returns topically relevant entries |
| `test_search_respects_k` | Returns at most k results |
| `test_search_filters_topic` | Topic filter narrows results |
| `test_search_filters_category` | Category filter narrows results |
| `test_search_excludes_deprecated` | Deprecated entries excluded by default |
| `test_search_excludes_quarantined` | Quarantined entries never in search results |
| `test_search_empty_query_rejected` | Empty query string → error |
| `test_search_no_results` | Query with no matches → empty results, no error |
| `test_search_format_summary` | format=summary returns expected structure |
| `test_search_format_markdown` | format=markdown returns expected structure |
| `test_search_format_json` | format=json returns parseable JSON |
| `test_search_reranking` | High-confidence entries rank above low-confidence (with similar similarity) |

#### context_lookup (~10 tests)
| Test | What it validates |
|------|-------------------|
| `test_lookup_by_id` | Single ID lookup returns exact entry |
| `test_lookup_by_topic` | Topic filter returns matching entries |
| `test_lookup_by_category` | Category filter returns matching entries |
| `test_lookup_by_tags` | Tag filter returns matching entries |
| `test_lookup_by_status` | Status filter works (active, deprecated, proposed) |
| `test_lookup_combined_filters` | Multiple filters intersect correctly |
| `test_lookup_no_match` | No matches → empty results, no error |
| `test_lookup_respects_limit` | Returns at most limit results |
| `test_lookup_excludes_quarantined_default` | Quarantined excluded unless explicitly requested |
| `test_lookup_format_json` | JSON format returns structured data |

#### context_get (~6 tests)
| Test | What it validates |
|------|-------------------|
| `test_get_existing` | Returns full entry metadata |
| `test_get_nonexistent` | Non-existent ID → clear error |
| `test_get_quarantined_visible` | Quarantined entries visible via get |
| `test_get_includes_all_metadata` | Response has confidence, usage counts, timestamps |
| `test_get_format_json` | JSON format returns complete structured data |
| `test_get_negative_id_rejected` | Negative ID → validation error |

#### context_correct (~8 tests)
| Test | What it validates |
|------|-------------------|
| `test_correct_creates_new_deprecates_old` | Original deprecated, new entry created |
| `test_correct_preserves_chain` | New entry links back to original via correction_chain_id |
| `test_correct_atomic` | Both operations in single transaction (no partial state) |
| `test_correct_nonexistent_original` | Correcting non-existent entry → error |
| `test_correct_already_deprecated` | Correcting already-deprecated entry → error or idempotent |
| `test_correct_content_scanned` | New content goes through content scanning |
| `test_correct_requires_write` | Restricted agent → capability error |
| `test_correct_chain_depth_3` | Correct a correction — chain depth > 2 works |

#### context_deprecate (~5 tests)
| Test | What it validates |
|------|-------------------|
| `test_deprecate_sets_status` | Entry status becomes Deprecated |
| `test_deprecate_idempotent` | Deprecating already-deprecated entry doesn't error |
| `test_deprecate_nonexistent` | Deprecating non-existent entry → error |
| `test_deprecate_requires_write` | Restricted agent → capability error |
| `test_deprecate_excluded_from_search` | Deprecated entry no longer in search results |

#### context_status (~8 tests)
| Test | What it validates |
|------|-------------------|
| `test_status_empty_db` | Status report on empty database doesn't error |
| `test_status_counts_accurate` | Entry counts match actual database state |
| `test_status_category_distribution` | Category breakdown matches stored entries |
| `test_status_topic_distribution` | Topic breakdown matches stored entries |
| `test_status_correction_chains` | Correction chains reported correctly |
| `test_status_confidence_distribution` | Confidence stats (min, max, mean) accurate |
| `test_status_format_json` | JSON format returns structured report |
| `test_status_embedding_check` | check_embeddings=true triggers re-embedding validation |

#### context_briefing (~8 tests)
| Test | What it validates |
|------|-------------------|
| `test_briefing_returns_content` | Briefing produces non-empty markdown |
| `test_briefing_role_filtering` | Different roles get different results |
| `test_briefing_task_relevance` | Task description influences which entries surface |
| `test_briefing_feature_boost` | Feature parameter boosts feature-specific entries |
| `test_briefing_max_tokens_respected` | Output stays within max_tokens budget |
| `test_briefing_excludes_quarantined` | Quarantined entries never in briefings |
| `test_briefing_empty_db` | Briefing on empty database doesn't error |
| `test_briefing_format_json` | JSON format returns structured briefing |

#### context_quarantine (~8 tests)
| Test | What it validates |
|------|-------------------|
| `test_quarantine_sets_status` | Entry status becomes Quarantined |
| `test_quarantine_excludes_from_search` | Quarantined entry gone from search |
| `test_quarantine_excludes_from_lookup` | Quarantined entry gone from default lookup |
| `test_quarantine_visible_via_get` | Quarantined entry still accessible by ID |
| `test_restore_reverses_quarantine` | Restore brings entry back to search/lookup |
| `test_quarantine_requires_admin` | Non-admin agent → capability error |
| `test_quarantine_nonexistent` | Quarantining non-existent entry → error |
| `test_quarantine_confidence_drops` | Quarantined entry base_score drops to 0.1 |

### Suite 3: Knowledge Lifecycle (`test_lifecycle.py`) — ~25 tests

Multi-step scenarios that exercise knowledge management workflows.

| Test | Scenario |
|------|----------|
| `test_store_search_find` | Store entry → search by similar query → find it |
| `test_store_correct_chain` | Store A → correct A with B → correct B with C → verify chain integrity |
| `test_store_deprecate_invisible` | Store → deprecate → verify invisible in search, visible in get |
| `test_store_quarantine_restore` | Store → quarantine → verify gone → restore → verify back |
| `test_confidence_increases_with_access` | Store → access N times → verify confidence increases |
| `test_confidence_helpfulness` | Store → vote helpful 10x → verify helpfulness factor increases |
| `test_confidence_unhelpful_decreases` | Store → vote unhelpful → verify confidence decreases vs baseline |
| `test_confidence_wilson_min_votes` | <5 votes → helpfulness factor stays neutral (0.5) |
| `test_usage_tracking_accurate` | Store → search 5x → verify access_count = 5 |
| `test_usage_dedup_same_session` | Multiple accesses in one search → single count increment |
| `test_feature_entries_tracked` | Store with feature_cycle → verify feature-entry linkage |
| `test_briefing_evolves_with_data` | Store domain entries → briefing reflects them |
| `test_contradiction_detected` | Store contradicting entries → status reports contradiction |
| `test_correction_chain_integrity` | Build 5-deep chain → get each entry → verify chain links |
| `test_status_reflects_mutations` | Store/correct/deprecate → status counts match |
| `test_search_reranking_effect` | High-confidence entry with moderate similarity beats low-confidence with high similarity |
| `test_multi_agent_access_patterns` | Multiple agent_ids store and retrieve → verify per-agent audit trail |
| `test_category_distribution_shift` | Store many entries across categories → status distribution accurate |
| `test_server_restart_data_persists` | Store entries → shutdown → restart → data present |
| `test_server_restart_vector_index_persists` | Store entries → shutdown → restart → search still works |
| `test_agent_auto_enrollment` | Unknown agent_id → auto-enrolled as Restricted |
| `test_restricted_agent_read_only` | Restricted agent can search/get but not store/correct |
| `test_privileged_agent_full_access` | Privileged agent can do everything |
| `test_audit_log_completeness` | Every tool call → audit entry with correct operation |
| `test_end_to_end_feature_cycle` | Simulate full feature: store decisions → briefing → access → confidence → status |

### Suite 4: Volume & Stress (`test_volume.py`) — ~15 tests

Scale testing. Shared server fixture to accumulate data.

| Test | What it validates |
|------|-------------------|
| `test_store_1000_entries` | 1K entries stored without error |
| `test_store_5000_entries` | 5K entries stored, timing recorded |
| `test_search_at_1000_entries` | Search returns relevant results in large index |
| `test_search_at_5000_entries` | Search still performant at 5K entries |
| `test_search_accuracy_at_scale` | Known-relevant entry found in top-5 among 5K entries |
| `test_lookup_at_scale` | Lookup by topic returns correct subset of 5K entries |
| `test_status_at_scale` | Status report computes over 5K entries without timeout |
| `test_briefing_at_scale` | Briefing selects from 5K entries within token budget |
| `test_concurrent_stores` | 50 sequential stores (simulating burst) all succeed |
| `test_large_content_1mb` | Single entry with ~1M char content stored and retrieved |
| `test_large_content_search` | Large-content entry findable via search |
| `test_many_topics_100` | 100 distinct topics, lookup each works |
| `test_many_categories` | All 8 categories populated, status distribution correct |
| `test_many_tags_per_entry` | Entry with 10 tags stored and searchable |
| `test_contradiction_scan_at_scale` | Contradiction scan over 1K entries completes without timeout |

### Suite 5: Security & Hardening (`test_security.py`) — ~30 tests

Validates defense layers.

#### Content Scanning (~10 tests)
| Test | What it validates |
|------|-------------------|
| `test_sql_injection_detected` | `'; DROP TABLE entries; --` flagged |
| `test_shell_injection_detected` | `` `rm -rf /` `` flagged |
| `test_template_injection_detected` | `{{constructor.constructor('return this')()}}` flagged |
| `test_prompt_injection_detected` | `Ignore previous instructions and...` flagged |
| `test_ssn_pii_detected` | Social Security Number pattern flagged |
| `test_api_key_detected` | `sk-...` API key pattern flagged |
| `test_private_key_detected` | `BEGIN PRIVATE KEY` flagged |
| `test_email_pii_detected` | Email address pattern flagged |
| `test_benign_content_not_flagged` | Normal technical content passes scanning |
| `test_scanning_on_correct` | Content scanning applies to corrections too |

#### Capability Enforcement (~8 tests)
| Test | What it validates |
|------|-------------------|
| `test_restricted_cannot_store` | Restricted agent → Write capability denied |
| `test_restricted_cannot_correct` | Restricted agent → Write capability denied |
| `test_restricted_cannot_deprecate` | Restricted agent → Write capability denied |
| `test_restricted_cannot_quarantine` | Restricted agent → Admin capability denied |
| `test_restricted_can_search` | Restricted agent → Search allowed |
| `test_restricted_can_lookup` | Restricted agent → Read allowed |
| `test_restricted_can_get` | Restricted agent → Read allowed |
| `test_unknown_agent_restricted` | Never-seen agent_id → auto-enrolled Restricted |

#### Input Validation (~12 tests)
| Test | What it validates |
|------|-------------------|
| `test_topic_max_length` | >100 char topic → rejected |
| `test_category_max_length` | >50 char category → rejected |
| `test_content_max_length` | >1M char content → rejected |
| `test_tag_max_length` | >100 char tag → rejected |
| `test_tag_max_count` | >10 tags → rejected |
| `test_control_chars_rejected` | `\x00`, `\x01` in topic → rejected |
| `test_null_bytes_rejected` | Null bytes in content → rejected |
| `test_negative_id_rejected` | Negative entry ID → validation error |
| `test_zero_k_rejected` | k=0 in search → validation error |
| `test_k_too_large_clamped` | k=10000 → clamped or rejected |
| `test_max_tokens_below_range` | max_tokens=100 (below 500) → rejected |
| `test_max_tokens_above_range` | max_tokens=50000 (above 10000) → rejected |

### Suite 6: Confidence System (`test_confidence.py`) — ~20 tests

Validates the 6-factor confidence formula and search re-ranking.

| Test | What it validates |
|------|-------------------|
| `test_base_score_active` | Active entry base_score = 0.5 |
| `test_base_score_deprecated` | Deprecated entry base_score = 0.2 |
| `test_base_score_proposed` | Proposed entry base_score = 0.2 |
| `test_base_score_quarantined` | Quarantined entry base_score = 0.1 |
| `test_usage_factor_increases_with_access` | More accesses → higher usage factor |
| `test_usage_factor_log_transform` | Usage factor follows log curve, not linear |
| `test_freshness_factor_decreases_over_time` | Older entries have lower freshness |
| `test_helpfulness_neutral_under_5_votes` | <5 total votes → helpfulness = 0.5 |
| `test_helpfulness_increases_with_helpful_votes` | 10 helpful, 0 unhelpful → high helpfulness |
| `test_helpfulness_decreases_with_unhelpful_votes` | 0 helpful, 10 unhelpful → low helpfulness |
| `test_helpfulness_wilson_lower_bound` | Wilson score, not naive ratio |
| `test_correction_factor_for_corrected_entry` | Entry that was corrected gets correction factor |
| `test_trust_factor_human_higher` | trust_source=human → higher trust factor |
| `test_confidence_composite_formula` | Known inputs → expected composite output |
| `test_confidence_range_0_to_1` | Confidence always in [0.0, 1.0] |
| `test_search_rerank_blends_similarity_confidence` | 0.85×sim + 0.15×conf applied |
| `test_search_rerank_order` | High-confidence entry beats higher-similarity low-confidence |
| `test_lookup_not_reranked` | Lookup results not affected by confidence |
| `test_get_not_reranked` | Get results not affected by confidence |
| `test_confidence_recomputed_on_quarantine` | Quarantine triggers confidence recomputation |

### Suite 7: Contradiction Detection (`test_contradiction.py`) — ~15 tests

Validates the contradiction scan and quarantine pipeline.

| Test | What it validates |
|------|-------------------|
| `test_negation_opposition_detected` | "always use X" vs "never use X" → flagged |
| `test_incompatible_directives_detected` | "use library A" vs "use library B for same purpose" → flagged |
| `test_opposing_sentiment_detected` | Positive vs negative assessment of same topic → flagged |
| `test_similar_but_compatible_not_flagged` | Related but non-contradicting entries → no flag |
| `test_dissimilar_entries_not_compared` | Low-similarity entries not paired |
| `test_contradiction_in_status_report` | Flagged contradictions appear in context_status |
| `test_contradiction_threshold_tunable` | Lower threshold → more flags, higher → fewer |
| `test_quarantine_removes_from_search` | Quarantined contradicting entry gone from search |
| `test_embedding_consistency_check` | Re-embed and compare detects drift |
| `test_embedding_consistency_opt_in` | check_embeddings defaults to false |
| `test_contradiction_across_topics` | Same-topic contradictions flagged, cross-topic not |
| `test_contradiction_with_correction_chain` | Corrected entries don't contradict their successors |
| `test_false_positive_resistance` | Normal technical variations not flagged |
| `test_many_contradictions_at_scale` | 50 contradicting pairs → all detected, no missed |
| `test_quarantine_restore_clears_flag` | Restore + resolve → no longer flagged |

### Suite 8: Edge Cases (`test_edge_cases.py`) — ~25 tests

Boundary conditions, unusual inputs, failure modes.

| Test | What it validates |
|------|-------------------|
| `test_empty_database_search` | Search on empty DB → empty results, no error |
| `test_empty_database_lookup` | Lookup on empty DB → empty results, no error |
| `test_empty_database_status` | Status on empty DB → zeros, no error |
| `test_empty_database_briefing` | Briefing on empty DB → minimal response, no error |
| `test_unicode_cjk_content` | Chinese/Japanese/Korean characters stored and searchable |
| `test_unicode_emoji_content` | Emoji in content stored and retrieved correctly |
| `test_unicode_rtl_content` | Right-to-left text (Arabic/Hebrew) handled |
| `test_unicode_combining_chars` | Combining characters preserved |
| `test_unicode_zwj_sequences` | Zero-width joiner sequences preserved |
| `test_max_length_title` | Title at exactly max length succeeds |
| `test_max_length_topic` | Topic at exactly 100 chars succeeds |
| `test_whitespace_only_content` | Content with only whitespace → rejected or handled |
| `test_very_long_tag_list` | Exactly 10 tags succeeds, 11 rejected |
| `test_duplicate_store_same_content` | Exact same content stored twice → near-duplicate warning |
| `test_rapid_sequential_stores` | 100 stores in tight loop all succeed |
| `test_search_special_query_chars` | Query with quotes, brackets, etc. doesn't crash |
| `test_lookup_all_statuses` | Each status value (active, deprecated, proposed, quarantined) filterable |
| `test_get_after_correct_returns_deprecated` | Getting corrected entry returns it with Deprecated status |
| `test_correct_then_correct_again` | Correcting a correction works |
| `test_deprecate_then_correct` | Can't correct an already-deprecated entry (or can?) |
| `test_store_minimal_1_char_content` | Single character content accepted |
| `test_all_formats_all_tools` | Every tool × every format (summary, markdown, json) produces valid output |
| `test_concurrent_store_and_search` | Store and search interleaved don't corrupt state |
| `test_server_handles_sigterm` | SIGTERM → clean shutdown, data persisted |
| `test_restart_preserves_agent_registry` | Agent registry survives restart |

---

## Test Data Fixtures

### `injection_patterns.json`
~50 prompt injection payloads covering:
- SQL injection variants
- Shell command injection
- Template injection (Jinja2, Handlebars, etc.)
- LLM prompt injection ("ignore previous instructions")
- LDAP injection
- XML/XXE injection
- Path traversal
- Unicode homoglyphs for bypass attempts

### `pii_samples.json`
PII test cases with expected detection:
- SSN patterns (XXX-XX-XXXX)
- Email addresses
- Phone numbers
- API keys (OpenAI, AWS, GitHub)
- Private keys (RSA, Ed25519)
- Passwords in config snippets
- Credit card numbers

### `unicode_corpus.json`
Unicode edge cases:
- CJK Unified Ideographs
- Emoji (single, ZWJ sequences, skin tones)
- RTL text (Arabic, Hebrew)
- Combining diacritical marks
- Mathematical symbols
- Surrogate pair boundaries
- Null byte variations
- BOMs and unusual whitespace (NBSP, zero-width space)

### `large_entries.json`
Pre-built large payloads:
- 100KB content
- 500KB content
- ~1MB content (near limit)
- Content with many paragraphs (10K+ lines)
- Content with deeply nested markdown

---

## Running Specific Suites

The `run.sh` entrypoint supports suite selection:

```bash
# All suites
TEST_SUITE=all ./scripts/run.sh

# Single suite
TEST_SUITE=protocol ./scripts/run.sh
TEST_SUITE=tools ./scripts/run.sh
TEST_SUITE=lifecycle ./scripts/run.sh
TEST_SUITE=volume ./scripts/run.sh
TEST_SUITE=security ./scripts/run.sh
TEST_SUITE=confidence ./scripts/run.sh
TEST_SUITE=contradiction ./scripts/run.sh
TEST_SUITE=edge_cases ./scripts/run.sh

# Multiple suites
TEST_SUITE=protocol,tools,security ./scripts/run.sh

# With pytest markers
TEST_SUITE=all PYTEST_ARGS="-m 'not slow'" ./scripts/run.sh
```

### pytest Markers

```ini
[pytest]
markers =
    slow: marks tests that take >10s (deselect with '-m "not slow"')
    volume: marks volume/stress tests
    security: marks security validation tests
    smoke: marks minimal smoke tests for quick validation
```

### Smoke Test (~30s)

A `smoke` marker on ~15 critical-path tests for quick validation:
- Store + get roundtrip
- Search finds stored entry
- Correct creates chain
- Quarantine excludes from search
- Content scanning catches injection
- Capability enforcement works
- Confidence is in valid range
- Status report works
- Briefing returns content
- Restart preserves data

```bash
TEST_SUITE=all PYTEST_ARGS="-m smoke" ./scripts/run.sh
```

---

## Output & Reporting

### Exit Codes
- `0`: All tests passed
- `1`: Test failures
- `2`: Test errors (infrastructure problems)

### Artifacts
```
/results/
├── junit.xml              # JUnit XML for CI systems
├── report.json            # Detailed JSON report (pytest-json-report)
├── summary.txt            # Human-readable summary
└── logs/
    └── server-{suite}.log # Server stderr logs per suite
```

### Summary Format
```
═══════════════════════════════════════════
  UNIMATRIX TEST HARNESS RESULTS
═══════════════════════════════════════════
  Suite            Passed  Failed  Errors
  ─────────────────────────────────────────
  protocol            15       0       0
  tools               80       0       0
  lifecycle           25       0       0
  volume              15       0       0
  security            30       0       0
  confidence          20       0       0
  contradiction       15       0       0
  edge_cases          25       0       0
  ─────────────────────────────────────────
  TOTAL              225       0       0
  ─────────────────────────────────────────
  Unit tests (cargo)  745       0       0
═══════════════════════════════════════════
  RESULT: PASS
═══════════════════════════════════════════
```

---

## CI Integration

### GitHub Actions (future)

```yaml
name: Integration Tests
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Run integration tests
        run: |
          docker compose -f product/test/infra-001/docker-compose.yml \
            up --build --abort-on-container-exit
      - name: Upload results
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: test-results
          path: test-results/
```

---

## Implementation Phases

### Phase 1: Foundation
- Dockerfile (multi-stage build)
- docker-compose.yml
- `harness/client.py` — MCP JSON-RPC client
- `harness/conftest.py` — server lifecycle fixtures
- `suites/test_protocol.py` — MCP handshake tests
- `scripts/run.sh` — entrypoint
- Verify: `docker compose up` works end-to-end

### Phase 2: Core Tool Coverage
- `harness/generators.py` — test data factories
- `harness/assertions.py` — custom assertion helpers
- `suites/test_tools.py` — all 9 tools, happy + error paths
- `suites/test_edge_cases.py` — boundary conditions
- `fixtures/unicode_corpus.json`

### Phase 3: Lifecycle & Confidence
- `suites/test_lifecycle.py` — multi-step scenarios
- `suites/test_confidence.py` — 6-factor formula validation
- `suites/test_contradiction.py` — detection pipeline

### Phase 4: Security & Volume
- `suites/test_security.py` — content scanning, capabilities, input validation
- `suites/test_volume.py` — scale tests
- `fixtures/injection_patterns.json`
- `fixtures/pii_samples.json`
- `fixtures/large_entries.json`

### Phase 5: Polish
- `scripts/report.sh` — formatted summary
- pytest markers (smoke, slow, volume, security)
- JUnit XML output
- Server log capture

---

## Key Design Decisions

### D1: Python + pytest over Rust integration tests

**Why**: The harness tests the *binary* through its *protocol*, not internal APIs. Python excels at subprocess management, JSON-RPC, data generation, and test parameterization. pytest's fixture system maps perfectly to server lifecycle management. Keeps test harness independent of the Rust codebase — a true black-box test.

**Trade-off**: No compile-time type safety on test code. Mitigated by thorough assertion helpers and typed dataclasses.

### D2: One server per test (default)

**Why**: Test isolation. No state leakage between tests. Each test gets a fresh database in a temp directory. Startup cost is low (~200ms) because the embedding model is lazy-loaded.

**Exception**: Volume suite uses shared server (scope=module) because accumulating 5K entries per test is wasteful.

### D3: Subprocess over HTTP/SDK

**Why**: The server's only transport is stdio (MCP over stdin/stdout). There's no HTTP endpoint. The test harness must match how agents actually use it — spawning a process and piping JSON-RPC. This is the most realistic test possible.

### D4: Fixtures over random data

**Why**: Reproducible tests. Random data makes failures hard to reproduce. Generators use deterministic seeds. Static fixtures for security payloads ensure coverage doesn't drift.

**Exception**: Volume suite uses seeded random generation for bulk entries (seed logged on failure for reproduction).

### D5: Docker Compose over raw Docker

**Why**: Multi-service orchestration (builder + test-runner) with shared volumes, environment variable overrides, and easy teardown. `--abort-on-container-exit` gives CI-friendly behavior.

---

## Test Count Summary

| Suite | Tests | Focus |
|-------|-------|-------|
| Protocol | ~15 | MCP handshake, JSON-RPC compliance |
| Tools | ~80 | All 9 tools, every parameter, every error |
| Lifecycle | ~25 | Multi-step knowledge management flows |
| Volume | ~15 | Scale to 5K entries, large payloads |
| Security | ~30 | Injection, PII, capabilities, validation |
| Confidence | ~20 | 6-factor formula, re-ranking, Wilson score |
| Contradiction | ~15 | Detection pipeline, quarantine, false positives |
| Edge Cases | ~25 | Unicode, boundaries, concurrent ops, restart |
| **Total** | **~225** | **System-level validation** |
| Unit (existing) | 745+ | Component-level (cargo test) |
| **Grand Total** | **~970+** | **Full trust** |
