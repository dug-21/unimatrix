# Scope Risk Assessment: nan-001

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Format version 1 becomes the nan-002 import contract — any field naming, type encoding, or null representation mistake locks in a breaking change or lossy round-trip | High | Med | Architect should define a canonical field-name mapping (SQL column name -> JSON key) and type-encoding table upfront; spec should include a round-trip fidelity invariant |
| SR-02 | Direct SQL access (ADR-003) bypasses Store API type guarantees — schema drift between Store types and raw SQL column lists can silently drop or mis-type columns | Med | Med | Architect should derive the column list from a single source of truth (e.g., a const array or macro shared with schema migrations) |
| SR-03 | JSON number precision for f64 confidence values — serde_json uses finite-precision float serialization that may not round-trip exactly | Med | Low | Spec should require a precision guarantee (e.g., 15 significant digits) and test round-trip equality for edge-case floats |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Schema version coupling — export hardcodes v11 column set; any schema migration between now and delivery changes the column list, breaking export | Med | Med | Architect should parameterize table/column definitions so export tracks schema evolution automatically |
| SR-05 | "Deterministic output" (AC-14) may conflict with JSON key ordering — serde_json HashMap serialization is not deterministic by default | Low | High | Architect should use BTreeMap or explicit ordered serialization; spec should clarify byte-identical means key-order-stable |
| SR-06 | Scope excludes incremental export — for large knowledge bases (>1000 entries), full export may become a pain point faster than expected, pressuring scope creep | Low | Low | Accept for v1; note in format_version design that incremental could be a v2 concern |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Concurrent access with running MCP server — WAL mode allows concurrent reads, but if the server writes mid-export, rows from different tables may reflect different logical states | High | Med | Architect should consider wrapping the export in a single SQLite read transaction (BEGIN DEFERRED / SNAPSHOT isolation) |
| SR-08 | Store::open() runs migrations on open — exporting a database currently served by a running server means two processes may race on migration | Low | Low | Spec should clarify: if server is running, migration already happened; if not, export opens and migrates safely alone |
| SR-09 | CLI subcommand shares the binary with MCP server — changes to main.rs clap structure or shared initialization code could regress server startup | Low | Med | Architect should keep export module self-contained with minimal shared code paths beyond path resolution |

## Assumptions

1. **Schema v11 is stable** (SCOPE.md §Constraints.1) — If any schema migration lands before nan-001 delivery, the column list and table set must be updated. Risk: SR-04.
2. **All 8 tables have no BLOB columns** (SCOPE.md §Format Design) — If any table gains a BLOB column before delivery, export needs base64 or exclusion logic. Currently safe.
3. **serde_json is sufficient for all SQL types** (SCOPE.md §Constraints.3) — JSON-encoded string columns in agent_registry (capabilities, allowed_topics) must be emitted as raw JSON strings, not double-encoded. Risk: SR-01.
4. **WAL mode provides adequate read isolation** (SCOPE.md §Constraints.4) — Without an explicit read transaction, snapshot isolation is per-statement, not per-export. Risk: SR-07.

## Design Recommendations

1. **(SR-07)** Wrap entire export in a single `BEGIN DEFERRED` transaction to get consistent snapshot across all 8 tables. This is the highest-priority architectural concern.
2. **(SR-01, SR-03)** Define the JSONL field contract explicitly in the spec — column-to-key mapping, null encoding, float precision, and JSON-string-in-JSON handling. This is the format nan-002 will depend on.
3. **(SR-02, SR-04)** Derive export column lists from a shared definition rather than hardcoding SQL strings, so schema changes automatically propagate.
4. **(SR-05)** Use ordered map serialization to guarantee deterministic output without relying on HashMap iteration order.
