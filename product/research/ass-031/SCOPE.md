# ASS-031: W3-1 Pre-Implementation Research Spike

**Status**: Complete
**Date**: 2026-03-24
**Feeds**: W3-1 delivery scoping and design
**Predecessor**: ASS-029 (scope definition, not executed)

---

## Summary

This spike executes the research scope defined in ASS-029. It answers all seven research questions before W3-1 delivery begins.

**The key finding that changes everything**: `unimatrix-learn` and `unimatrix-adapt` crates already exist with production-quality pure-Rust MLP training infrastructure (`ndarray`-based, `bincode` serialized, EWC++ regularization, reservoir sampling, three-slot model registry). W3-1 does not need to build ML infrastructure from scratch — it extends what is already there.

**The second key finding**: `category_counts` (the WA-2 histogram) is never persisted to the database. It is in-memory only and reset on session reconnection. Historical sessions cannot have their full session context reconstructed from DB records alone. This is a **prerequisite defect that must be addressed in W3-1** via a new `session_category_snapshots` table.

---

## Research Questions — Disposition

| Question | Status | Document |
|---|---|---|
| Q1: Forward pass architecture | Resolved | `GNN-ARCHITECTURE.md` |
| Q2: Session context feature vector | Resolved with gap identified | `FEATURE-SPEC.md` |
| Q3: Entry feature vector | Resolved | `FEATURE-SPEC.md` |
| Q4: Training batch construction | Resolved with prerequisite | `TRAINING-DESIGN.md` |
| Q5: Candidate set management for proactive mode | Resolved | `GNN-ARCHITECTURE.md` |
| Q6: Tick scheduling and resource envelope | Resolved | `TICK-DESIGN.md` |
| Q7: Cold-start and fallback | Resolved | `GNN-ARCHITECTURE.md` |

---

## Relationship to Planned Work

| Item | Relationship |
|---|---|
| `unimatrix-learn` (existing) | Extend — do not replace. `NeuralModel` trait, `TrainingReservoir`, `ModelRegistry` are all direct building blocks. |
| `unimatrix-adapt` (existing) | Reference pattern only. MicroLoRA is for embedding adaptation; W3-1 is a scoring head, not an embedding adaptation. |
| WA-2 (category_histogram) | category_counts in SessionState is the live signal; `session_category_snapshots` table is the training data prerequisite. |
| W1-5 (col-023) | Behavioral outcome signals feed the implicit training label pipeline. W3-1 training benefits but does not require W1-5 completion. Explicit helpfulness votes and session-close signals (already in signal_queue) are sufficient for initial training. |
| WA-3 (MissedRetrieval) | Deferred. MISSED_RETRIEVALS table does not exist. W3-1 training design accounts for this absence — explicit and implicit labels are sufficient for the first training run. Revisit after W3-1 ships. |
| `w_phase_explicit = 0.0` (crt-026) | This is W3-1's hook in the scoring formula. The GNN affinity score replaces the WA-2 manual affinity terms (phase_histogram + phase_explicit). |
| W1-3 eval harness | Gate condition for W3-1 promotion from shadow to production. |

---

## What This Spike Does Not Resolve

See `OPEN-QUESTIONS.md` for decisions deferred to W3-1 delivery:

- Exact phase vocabulary extraction procedure
- `session_category_snapshots` retention window
- Minimum viable training set threshold tuning
- Whether Mode 3 query signal injection is by concatenation or separate head
- Eval harness integration specifics

---

## Effort Validation

The ASS-029 scope estimated "1-2 weeks (no GNN infrastructure exists)". That estimate is now invalidated. With `unimatrix-learn` already providing `NeuralModel`, `TrainingReservoir`, `ModelRegistry`, and serialization, the effort estimate is:

| Component | Effort |
|---|---|
| `RelevanceDigest` feature vector construction | 0.5 day |
| `RelevanceScorer` model implementation | 1 day |
| Training service extension (new model registration) | 0.5 day |
| `session_category_snapshots` schema migration + write path | 0.5 day |
| Phase transition cache replacement (GNN scoring replaces manual formula) | 1 day |
| Tick integration (training gate, model reload) | 0.5 day |
| Cold-start blend alpha activation | 0.5 day |
| Integration tests + eval harness gate | 1 day |
| **Total** | **~5-6 days** |

This is a significant reduction from the 1-2 week estimate. The infrastructure investment in `unimatrix-learn` and `unimatrix-adapt` pays off here.
