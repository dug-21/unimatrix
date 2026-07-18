# Agent Report: vnc-048-researcher

**Phase**: 1 (problem space exploration / scope)
**Deliverable**: `product/features/vnc-048/SCOPE.md`

## Summary

Scoped the capability (per-slug backup/restore end-to-end), not the export flag. Resolved the human's main open question — import's `paths.vector_dir` rebuild — decisively: mechanically a one-line delta, but it exposes a design problem export does not have (live-daemon shutdown clobbers the rebuilt vector index). 13 ACs, 10 constraints, 5 open questions (OQ-1 is blocking and behavior-changing).

## Key findings

1. **Import vector rebuild = small delta + a real design problem.** Per-slug layout is relatively identical to hash layout (`{dir}/unimatrix.db` + `{dir}/vector`); `project register` already creates the vector dir. But `infra/shutdown.rs:189-197` dumps `per_slug_vectors` at daemon shutdown, so a live-daemon import has its rebuilt index overwritten by the daemon's stale boot-time index. DB survives, vector search silently reverts.
2. **The restore sequence is the product outcome**: `register → import --slug → restart`. It avoids the clobber by construction and is already what `project register` prints.
3. **C-5 (new, not previously surfaced)**: `insert_audit_log` is a plain INSERT with explicit event_id; `drop_all_data` cannot clear `audit_log` (append-only triggers). Restoring into a slug store with audit history hits a UNIQUE collision even with `--force`. Supported target = freshly-registered, audit-empty slug store.
4. **#5586 assessment**: table-level losslessness genuinely proven; *which database* never tested — all tests seed and read through the same hash resolver. Proven for local single-project, unproven for the cloud shape. `proven_by` does not name its resolver, which is what let a resolver-blind test read as full proof.
5. Confirmed the two decided constraints held: `--skip-quarantined` out of scope, base = `data_dir.parent()` with no new mechanism.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` (vnc-048) — surfaced the `personal-cloud` capability cluster (#5582/#5589/#5591/#5594) and container/config placement patterns (#4626, #5248); `context_get(5586)` for the BACKUP-RESTORE `done_when`/`proven_by`. Prior-agent findings carried forward: #4972, #5507, #80, #1192, #4577, #5270.
- Stored: nothing novel to store — the generalizable insight (path-hash vs literal-slug resolver divergence) is already covered by #4972 and #5507; the shutdown-clobber finding is feature-specific and belongs in SCOPE.md; the defect itself is a GH issue per project policy (bugs are GH issues, not lessons). If the clobber generalizes beyond vnc-048 at delivery, it is a pattern for the retro to store on evidence.
