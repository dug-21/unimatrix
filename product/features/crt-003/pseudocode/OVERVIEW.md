# Pseudocode Overview: crt-003

## Component Interaction

```
C1 (status-extension) ─── cross-crate enum change
  │
  ├──► C2 (retrieval-filtering) ── modifies tool handlers in tools.rs
  │      └── adds Quarantined status check in search/lookup/correct result paths
  │
  ├──► C3 (quarantine-tool) ── new tool handler + server methods
  │      ├── tools.rs: context_quarantine handler
  │      ├── server.rs: quarantine_with_audit(), restore_with_audit()
  │      ├── validation.rs: validate_quarantine_params(), parse_action()
  │      └── response.rs: format_quarantine_success(), format_restore_success()
  │
  ├──► C4 (contradiction-detection) ── new module contradiction.rs
  │      ├── scan_contradictions() ── main scanning function
  │      ├── check_embedding_consistency() ── opt-in integrity check
  │      ├── conflict_heuristic() ── multi-signal rule-based detection
  │      └── helper functions for negation, directive, sentiment detection
  │
  └──► C5 (status-report-extension) ── extends existing status tool
         ├── StatusReport gains quarantine/contradiction/embedding fields
         ├── StatusParams gains check_embeddings parameter
         ├── context_status handler calls C4 scanning functions
         └── format_status_report extended with new sections

## Shared Types

- `Status::Quarantined` ── new variant in unimatrix-store/src/schema.rs (C1)
- `ContradictionPair` ── struct in contradiction.rs (C4, consumed by C5)
- `EmbeddingInconsistency` ── struct in contradiction.rs (C4, consumed by C5)
- `ContradictionConfig` ── struct in contradiction.rs (C4, consumed by C5)

## Data Flow

1. Entry lifecycle: Active ─(quarantine)─► Quarantined ─(restore)─► Active
2. Retrieval: HNSW search → metadata filter → **status filter (new)** → format response
3. Contradiction scan: iterate ENTRIES → re-embed → search HNSW → conflict heuristic → report
4. Embedding check: iterate ENTRIES → re-embed → search HNSW self-match → compare → report
