# Test Plan: C6 — Test Suites

## Suite 1: Protocol (~15 tests)

| ID | Test | Risk | Marker |
|----|------|------|--------|
| P-01 | Initialize returns capabilities | R-04 | smoke |
| P-02 | Server info (name=unimatrix, version) | — | smoke |
| P-03 | List tools returns 9 context_* tools | — | |
| P-04 | Tool inputSchemas are valid JSON Schema objects | — | |
| P-05 | Unknown tool returns error | R-01 | |
| P-06 | Malformed JSON-RPC handled gracefully | R-01 | |
| P-07 | Missing required params returns error | R-01 | |
| P-08 | Rapid sequential requests get correct responses | R-01 | |
| P-09 | Notifications don't produce responses | R-01 | |
| P-10 | Graceful shutdown with clean exit | R-02 | smoke |
| P-11 | Invalid bytes don't crash server | R-01 | |
| P-12 | Large request payload handled | R-01 | |
| P-13 | Empty arguments handled per defaults | — | |
| P-14 | Unknown fields ignored | — | |
| P-15 | JSON format responses are parseable JSON | R-03 | |

## Suite 2: Tools (~80 tests)

### context_store (15 tests)
| ID | Test | Risk |
|----|------|------|
| T-01 | Store minimal (content + topic + category) | — |
| T-02 | Store all fields | — |
| T-03 | Store roundtrip (store then get, fields match) | R-03 |
| T-04 | Store near-duplicate detection | — |
| T-05 | Store invalid category rejected | — |
| T-06 | Store empty content rejected | — |
| T-07 | Store empty topic rejected | — |
| T-08 | Store restricted agent rejected (no Write) | — |
| T-09 | Store injection content rejected | R-06 |
| T-10 | Store PII content rejected | R-06 |
| T-11 | Store with tags (1-10) | — |
| T-12 | Store with >10 tags rejected | — |
| T-13 | Store format=json returns entry data | R-03 |
| T-14 | Store format=markdown returns markdown | R-03 |
| T-15 | Store format=summary returns text | R-03 |

### context_search (12 tests)
| ID | Test | Risk |
|----|------|------|
| T-16 | Search returns relevant results | — |
| T-17 | Search with topic filter | — |
| T-18 | Search with category filter | — |
| T-19 | Search with tags filter | — |
| T-20 | Search with k limit | — |
| T-21 | Search excludes deprecated entries | — |
| T-22 | Search excludes quarantined entries | — |
| T-23 | Search all three formats | R-03 |
| T-24 | Search with confidence re-ranking observable | — |
| T-25 | Search empty query returns error | — |
| T-26 | Search with feature parameter | — |
| T-27 | Search with helpful parameter | — |

### context_lookup (10 tests)
| ID | Test | Risk |
|----|------|------|
| T-28 | Lookup by topic | — |
| T-29 | Lookup by category | — |
| T-30 | Lookup by ID | — |
| T-31 | Lookup by tags | — |
| T-32 | Lookup by status | — |
| T-33 | Lookup combined filters | — |
| T-34 | Lookup with limit | — |
| T-35 | Lookup all formats | R-03 |
| T-36 | Lookup empty filters returns all | — |
| T-37 | Lookup nonexistent topic returns empty | — |

### context_get (6 tests)
| ID | Test | Risk |
|----|------|------|
| T-38 | Get existing entry | — |
| T-39 | Get nonexistent ID errors | — |
| T-40 | Get quarantined entry (still visible) | — |
| T-41 | Get with metadata (confidence, timestamps) | — |
| T-42 | Get all formats | R-03 |
| T-43 | Get invalid ID (negative, zero) errors | — |

### context_correct (8 tests)
| ID | Test | Risk |
|----|------|------|
| T-44 | Correct creates chain (original deprecated, new created) | — |
| T-45 | Correct atomic (both changes in one transaction) | — |
| T-46 | Correct nonexistent ID errors | — |
| T-47 | Correct already-deprecated entry errors | — |
| T-48 | Correct with content scanning | R-06 |
| T-49 | Correct requires Write capability | — |
| T-50 | Correct preserves original metadata unless overridden | — |
| T-51 | Correct all formats | R-03 |

### context_deprecate (5 tests)
| ID | Test | Risk |
|----|------|------|
| T-52 | Deprecate changes status | — |
| T-53 | Deprecate idempotent | — |
| T-54 | Deprecate nonexistent errors | — |
| T-55 | Deprecate requires Write capability | — |
| T-56 | Deprecated excluded from default search | — |

### context_status (8 tests)
| ID | Test | Risk |
|----|------|------|
| T-57 | Status empty database | — |
| T-58 | Status shows entry counts | — |
| T-59 | Status topic distribution | — |
| T-60 | Status category distribution | — |
| T-61 | Status correction chain info | — |
| T-62 | Status confidence stats | — |
| T-63 | Status all formats | R-03 |
| T-64 | Status with embedding check | — |

### context_briefing (8 tests)
| ID | Test | Risk |
|----|------|------|
| T-65 | Briefing returns content for role+task | — |
| T-66 | Briefing with feature filter | — |
| T-67 | Briefing with max_tokens | — |
| T-68 | Briefing quarantine exclusion | — |
| T-69 | Briefing empty database | — |
| T-70 | Briefing all formats | R-03 |
| T-71 | Briefing missing required params errors | — |
| T-72 | Briefing with helpful parameter | — |

### context_quarantine (8 tests)
| ID | Test | Risk |
|----|------|------|
| T-73 | Quarantine changes status | — |
| T-74 | Quarantined excluded from search | — |
| T-75 | Quarantined excluded from lookup (default) | — |
| T-76 | Quarantined visible via get | — |
| T-77 | Restore returns to active | — |
| T-78 | Quarantine requires Admin capability | — |
| T-79 | Quarantine confidence recomputed | — |
| T-80 | Quarantine all formats | R-03 |

## Suite 3: Lifecycle (~25 tests)

| ID | Test | Risk | Marker |
|----|------|------|--------|
| L-01 | Store -> search -> find flow | — | smoke |
| L-02 | Correction chain integrity (3-deep) | — | smoke |
| L-03 | Confidence evolution over access | — | |
| L-04 | Agent auto-enrollment on first request | — | |
| L-05 | Audit log completeness (store, search, correct, deprecate) | — | |
| L-06 | Test isolation: no state leakage between function-scoped tests | R-02, R-12 | smoke |
| L-07 | Store -> deprecate -> search doesn't find | — | |
| L-08 | Store -> quarantine -> restore -> search finds | — | |
| L-09 | Multi-agent interaction (different trust levels) | — | |
| L-10 | Correction chain with topic override | — | |
| L-11 | Full lifecycle: store, access, correct, deprecate, status | — | |
| L-12 | Data persistence across server restart | — | |
| L-13 | Feature cycle linkage (feature parameter) | — | |
| L-14 | Helpfulness voting (helpful=true/false) | — | |
| L-15 | Usage tracking deduplication | — | |
| L-16 | Store -> search -> get -> mark helpful -> search (re-ranking) | — | |
| L-17 | Briefing content reflects stored knowledge | — | |
| L-18 | Status report reflects lifecycle changes | — | |
| L-19 | Concurrent store from different topics | — | |
| L-20 | Deprecate -> correct (error: can't correct deprecated) | — | |
| L-21 | Quarantine -> correct (error: can't correct quarantined) | — | |
| L-22 | Multi-step correction chain (5 deep) | — | |
| L-23 | Agent enrollment persists across requests | — | |
| L-24 | Audit events for admin operations | — | |
| L-25 | Full pipeline: store 10 -> search -> correct 2 -> deprecate 1 -> status | — | |

## Suite 4: Volume (~15 tests)

| ID | Test | Risk | Marker |
|----|------|------|--------|
| V-01 | Store 1000 entries sequentially | R-07, R-11 | volume, slow |
| V-02 | Search accuracy at 1K entries | R-07 | volume |
| V-03 | Lookup correctness at 1K entries | — | volume |
| V-04 | Status report at 1K entries | — | volume |
| V-05 | Store 5000 entries | R-11 | volume, slow |
| V-06 | Search at 5K entries returns results | R-07 | volume |
| V-07 | 100 sequential search queries | R-09 | volume |
| V-08 | 100 distinct topics | — | volume |
| V-09 | Large content entry (100KB) | — | volume |
| V-10 | Large content entry (500KB) | — | volume |
| V-11 | Large content entry (~1MB) | R-11 | volume |
| V-12 | Contradiction scan at 1K entries | — | volume, slow |
| V-13 | Briefing with large knowledge base | — | volume |
| V-14 | Embedding consistency at scale | — | volume |
| V-15 | 100 rapid store-then-search pairs | R-09 | volume |

## Suite 5: Security (~30 tests)

| ID | Test | Risk | Marker |
|----|------|------|--------|
| S-01..S-10 | Injection pattern detection (from fixtures) | R-06 | security |
| S-11..S-18 | PII detection (from fixtures) | R-06 | security |
| S-19 | Restricted agent: search allowed | — | security |
| S-20 | Restricted agent: lookup allowed | — | security |
| S-21 | Restricted agent: store rejected | — | security |
| S-22 | Restricted agent: correct rejected | — | security |
| S-23 | Restricted agent: deprecate rejected | — | security |
| S-24 | Restricted agent: quarantine rejected | — | security |
| S-25 | Restricted agent: status rejected (Admin) | — | security |
| S-26 | Input validation: max content length | — | security |
| S-27 | Input validation: max topic length | — | security |
| S-28 | Input validation: control characters | — | security |
| S-29 | Input validation: invalid entry ID (negative) | — | security |
| S-30 | False positive: safe content accepted | R-06 | security |

## Suite 6: Confidence (~20 tests)

| ID | Test | Risk |
|----|------|------|
| C-01 | Base score for active entry | — |
| C-02 | Base score for deprecated entry | — |
| C-03 | Base score for quarantined entry | — |
| C-04 | Usage factor increases with access | — |
| C-05 | Freshness factor (new entry > old entry) | — |
| C-06 | Helpfulness: helpful=true increases score | — |
| C-07 | Helpfulness: helpful=false decreases score | — |
| C-08 | Wilson score with <5 votes (guard active) | — |
| C-09 | Wilson score with >5 votes (full formula) | — |
| C-10 | Correction factor (corrected entry lower) | — |
| C-11 | Trust factor (privileged agent > restricted) | — |
| C-12 | Search re-ranking: 0.85*sim + 0.15*conf | — |
| C-13 | Confidence visible in status report | — |
| C-14 | Confidence recomputed on quarantine | — |
| C-15 | Confidence recomputed on restore | — |
| C-16 | Confidence in JSON format response | — |
| C-17 | Multiple helpful votes accumulate | — |
| C-18 | Confidence range [0, 1] | — |
| C-19 | New entry default confidence | — |
| C-20 | Confidence after 10 searches (usage factor) | — |

## Suite 7: Contradiction (~15 tests)

| ID | Test | Risk |
|----|------|------|
| D-01 | Negation opposition detected ("always X" vs "never X") | R-06 |
| D-02 | Incompatible directives detected | R-06 |
| D-03 | Opposing sentiment detected | — |
| D-04 | False positive: compatible related entries not flagged | R-06 |
| D-05 | False positive: same-topic different-aspect entries | — |
| D-06 | Contradiction scan in status report | — |
| D-07 | Scan with sensitivity threshold | — |
| D-08 | Embedding consistency check | — |
| D-09 | Quarantine effect on contradiction scan | — |
| D-10 | Contradiction scan at 100 entries | — |
| D-11 | Generated contradicting pair triggers detection | R-06 |
| D-12 | Scan empty database | — |
| D-13 | Scan single entry | — |
| D-14 | Multiple contradiction pairs | — |
| D-15 | Scan with topic filter | — |

## Suite 8: Edge Cases (~25 tests)

| ID | Test | Risk | Marker |
|----|------|------|--------|
| E-01 | Unicode CJK roundtrip | R-10 | smoke |
| E-02 | Unicode Japanese roundtrip | — | |
| E-03 | Unicode Korean roundtrip | — | |
| E-04 | Unicode RTL Arabic roundtrip | — | |
| E-05 | Unicode emoji roundtrip | — | |
| E-06 | Unicode ZWJ sequences roundtrip | — | |
| E-07 | Unicode combining characters roundtrip | — | |
| E-08 | Empty database: all read tools return empty/zero | — | smoke |
| E-09 | Minimum-length fields (1-char content, 1-char topic) | — | |
| E-10 | Maximum-length topic (100 chars) | — | |
| E-11 | 10 tags on entry | — | |
| E-12 | Concurrent store operations (sequential, verify all stored) | R-10 | |
| E-13 | Server restart persistence (shutdown + re-connect, data exists) | R-02 | smoke |
| E-14 | Interleaved store and search | R-09 | |
| E-15 | Very long content (near boundary) | — | |
| E-16 | Special characters in query | — | |
| E-17 | Special characters in topic | — | |
| E-18 | Special characters in tags | — | |
| E-19 | Empty tags array | — | |
| E-20 | Null-like values in optional fields | — | |
| E-21 | 100 rapid sequential stores | R-09 | slow |
| E-22 | All formats x store/search/get/lookup | R-03 | |
| E-23 | Mixed RTL/LTR content roundtrip | — | |
| E-24 | Server process cleanup after fixture teardown | R-02 | smoke |
| E-25 | Store with source field roundtrip | — | |
