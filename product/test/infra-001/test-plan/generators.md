# Test Plan: C3 — Test Data Generators

## Scope

Generators produce deterministic test data. They are validated by the suites that consume their output. If a generator produces invalid data, the consuming test fails.

## Generator Validation Through Usage

| Generator | Consuming Suite | Validation |
|-----------|----------------|------------|
| make_entry | Tools (store roundtrip) | Server accepts and stores; fields match on retrieval |
| make_entries | Populated fixture, Volume | 50 entries stored successfully; bulk dataset stored |
| make_contradicting_pair | Contradiction | Server's contradiction detection flags the pair |
| make_correction_chain | Lifecycle | Correction chain created, IDs linked, status changes |
| make_injection_payloads | Security | Server content scanner rejects these payloads |
| make_pii_content | Security | Server PII scanner detects these |
| make_unicode_edge_cases | Edge Cases | Server stores and retrieves unicode content |
| make_bulk_dataset | Volume | 1K-5K entries stored and queryable |

## Determinism Tests

| Test | What It Validates |
|------|------------------|
| (Implicit) | Same seed produces same output across runs |
| (Implicit) | Seeds logged on test failure (FR-03.10) |

## Category Validity

All generators use categories from the server's allowlist:
- outcome, lesson-learned, decision, convention, pattern, procedure, duties, reference

If a generator produces an invalid category, the consuming `context_store` call fails with a clear error.

## Risk Coverage

| Risk | Generator Responsibility | Validation |
|------|------------------------|------------|
| R-06 | Produce inputs that exercise server behavior | Contradiction pair triggers detection; injection payloads detected |
| R-10 | Deterministic seeds for reproducibility | Seeds fixed per generator; logged on failure |
