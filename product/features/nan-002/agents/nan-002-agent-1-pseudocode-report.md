# Agent Report: nan-002-agent-1-pseudocode

## Task
Produce per-component pseudocode files for nan-002 (Knowledge Import).

## Components Covered

1. **cli-registration** -- `Command::Import` enum variant + match arm in `main.rs`
2. **format-types** -- Shared typed deserialization structs in `format.rs`
3. **import-pipeline** -- Full import pipeline in `import.rs`
4. **embedding-reconstruction** -- Re-embedding + VectorIndex building after DB commit

## Output Files

| File | Path |
|------|------|
| Overview | `product/features/nan-002/pseudocode/OVERVIEW.md` |
| CLI Registration | `product/features/nan-002/pseudocode/cli-registration.md` |
| Format Types | `product/features/nan-002/pseudocode/format-types.md` |
| Import Pipeline | `product/features/nan-002/pseudocode/import-pipeline.md` |
| Embedding Reconstruction | `product/features/nan-002/pseudocode/embedding-reconstruction.md` |

## Corrections Applied from Implementation Brief

1. `FeatureEntryRow.feature_id` (not `feature_cycle`) -- verified against DDL and export code.
2. `EntryRow` includes `source`, `correction_count`, `embedding_dim` -- Specification FR-06 was wrong.
3. Counter restoration uses `INSERT OR REPLACE INTO counters`.
4. Architecture Integration Surface lists `feature_cycle` for `FeatureEntryRow` -- this is incorrect per DDL and export code. Pseudocode uses `feature_id`.

## Open Questions

1. **Audit provenance event_id**: The import computes `MAX(event_id) + 1` from the audit_log table. If the export includes a counter for audit event IDs (e.g., `next_event_id`), using that counter would be more correct. However, no such counter appears in the current schema -- the audit_log uses auto-increment-style event_ids managed by the AuditLog struct. The `MAX + 1` approach is safe for import since all audit entries are restored before the provenance entry is written.

2. **drop_all_data scope**: The pipeline deletes from 9 tables (8 importable + vector_map). There are additional tables in the schema (sessions, observations, injection_log, query_log, signal_queue, observation_metrics, observation_phase_metrics, shadow_evaluations, topic_deliveries) that are NOT imported/exported. The --force operation does NOT clear these operational tables. If a user force-imports over a database that has operational data, those tables retain their data. This seems correct (operational data is separate from knowledge data) but is worth noting.

3. **embed_entries signature**: The pseudocode uses `embed_entries(&provider, &batch_input, ": ")` where `batch_input` is `Vec<(String, String)>`. The actual signature takes `&[(String, String)]`. Implementation agents should verify the exact generic bounds.

## Knowledge Stewardship

- Queried: export.rs for format contract patterns -- confirmed Value-based serialization, 8 tables, column ordering, JSON key names for feature_entries (`feature_id`) and outcome_index (`feature_cycle`)
- Queried: db.rs DDL for table schemas -- confirmed 26 entry columns, feature_entries uses `feature_id`, outcome_index uses `feature_cycle`
- Queried: main.rs for CLI subcommand pattern -- confirmed Hook/Export sync path, Command enum structure
- Queried: hash.rs for compute_content_hash -- confirmed edge-case behavior (empty title/content handling)
- Queried: project.rs for ProjectPaths struct -- confirmed fields (db_path, vector_dir, pid_path, data_dir)
- Deviations from established patterns: none. Import follows the same sync-path pattern as Export and Hook.

## Self-Check

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names -- every name traced to architecture or codebase
- [x] Output is per-component (OVERVIEW.md + one file per component), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO, placeholder functions, or TBD sections -- gaps flagged explicitly
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/nan-002/pseudocode/`
- [x] Knowledge Stewardship report block included
