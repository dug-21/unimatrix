## ADR-004: Server Precedence — `FeatureSource::{Declared, Inferred}` Classes, `apply_stamp`, and Both Declared-vs-Vote Inversion Flips

### Context

Two live inversion sites let the majority vote beat the declared feature (ass-072, "must land regardless of the stamp"): `sweep_stale_sessions` resolves `majority_vote_internal(..).or_else(|| state.feature.clone())` (infra/session.rs:627-628), and `process_session_close` uses vote → content-fallback → registry feature (listener.rs:1950-1978). Write-time, `enrich_topic_signal` (listener.rs:148-179) lets an extracted signal beat a declared registry feature unconditionally ("explicit wins", AC-08/FR-14 — the row-level #588 surface, 20.2% of live observations). The registry cannot distinguish a feature set by `cycle_start` (`set_feature_force`) from one set by eager vote (`set_feature_if_absent`) or SessionStart registration. SR-05 warns the "except where the precedence chain requires it" escape hatch must be fenced.

### Decision

1. **Precedence chain (AC-04), presence-gated — structurally un-invertable**:
   ```
   STAMP  (write-time)   event.cycle_stamp present → row attributes from the stamp
   MARKER (review-time)  tier named, NOT implemented here (deferred follow-up, ADR-007 §4)
   VOTE   (NULL-only)    enrich NULL-fill (write), eager (already NULL-only), close/sweep vote
                         — consulted ONLY where no Declared feature exists
   ```
2. **Registry types** (infra/session.rs):
   ```rust
   pub enum FeatureSource { Declared, Inferred(InferredOrigin) }
   pub enum InferredOrigin { Registered, Voted }
   // SessionState gains: pub feature_source: FeatureSource   // default Inferred(Registered)
   ```
   AC-04's two named variants are the **precedence classes** — all precedence checks are `matches!(src, FeatureSource::Declared)`. `InferredOrigin` exists solely so `topic_source` can distinguish registry-fill from vote-derived fill (ADR-005 / SR-04); it never affects precedence.
3. **Source assignment — exactly these sites, nothing else (SR-05 fence)**:
   - `set_feature_force` (cycle_start interception, both client types) → `Declared`.
   - **New** `SessionRegistry::apply_stamp(session_id, topic)` → `Declared`; called from the record paths when `cycle_stamp` is present; idempotent (no-op when feature+source already match; no `Overridden` log noise per event). This covers server restart mid-session: the first post-restart stamped event re-establishes `Declared` with no cycle_start replay.
   - `set_feature_if_absent` (eager, #198) → `Inferred(Voted)`.
   - `register_session` feature param (SessionStart `feature_cycle` extra) → `Inferred(Registered)`. The resume/compact overwrite resets to this — accepted; the next stamped event restores `Declared` (the stamp's whole point). Registry lifecycle redesign stays a named follow-up (ass-072 discoveries 2/3, #4140).
4. **Record-path semantics (all three sites, one shared helper)**:
   - `cycle_stamp` Some → `topic_signal := stamp.topic`; `phase := stamp.phase` else registry `current_phase`; `topic_source := 'declared'`; `apply_stamp(...)`; **skip** `record_topic_signal` tally (stamped events must not feed the vote) and **skip** `enrich_topic_signal`.
   - CYCLE_* events (any client) → `'declared'` rows (their `topic_signal` IS the declaration).
   - Otherwise → `enrich_topic_signal` extended with the FeatureSource guard:
     ```
     registry (feat, src) lookup (one get_state, as today)
     feat Some && src == Declared            → topic_signal := feat,      source 'declared'
         (declared registry beats extraction — the unstamped-window #588 remedy;
          the old "explicit wins" arm survives ONLY against Inferred/absent registry)
     else extracted Some                     → topic_signal := extracted, source 'extracted'
     else feat Some && src == Inferred(Registered) → fill,                source 'registry-fill'
     else feat Some && src == Inferred(Voted)      → fill,                source 'vote'
     else                                    → NULL,                      source NULL
     ```
5. **Inversion flip 1 — sweep** (session.rs:628): `resolved_feature = if matches!(state.feature_source, FeatureSource::Declared) && state.feature.is_some() { state.feature.clone() } else { majority_vote_internal(&state.topic_signals).or_else(|| state.feature.clone()) }`. Minimal diff by design (SR-10 — crt-052 rebases over this).
6. **Inversion flip 2 — close** (listener.rs): snapshot `feature_source` in the existing `get_state` capture (:1892); `final_feature_cycle = declared-and-present ? feature_cycle : resolved_topic → content_fallback → feature_cycle` (today's order, now reached only for non-Declared sessions).
7. **Demotion, not deletion**: extraction (unstamped sessions), `enrich_topic_signal` NULL-fill, eager, and close/sweep vote all survive — they are the only attribution for never-declare sessions (60% of sessions / 25% of observations). The only things retired are the two precedence bugs.

### Consequences

Easier: declared sessions can no longer be vote-inverted at any site — write-time or close-time, stamped or unstamped (the unstamped Rust-hook window is covered by the FeatureSource guard alone); the vote self-scopes to the residue class because stamped events neither emit extracted signals (ADR-002) nor feed tallies; crt-052 gets citable post-fix semantics (ADR-007 §2). Harder: `enrich_topic_signal`'s contract changes ("explicit wins" is no longer unconditional) — its doc comment and the AC-08/FR-14 references must be rewritten, and the debug-forensics log inverts (now logs when extraction is overridden by declared); `InferredOrigin` is one more concept, justified solely by the F6 evidence base (SR-04); per-event `apply_stamp` adds one mutex acquisition per stamped record (microseconds, same class as the existing `record_topic_signal` lock).

Cross-references: SCOPE AC-04, #588 (closed via this PR — disposition mapping in ARCHITECTURE.md), SR-05/SR-10, ass-072 Q4/Q5, #1067 (eager immutable, unchanged), #3382 (NULL-fill retained), #4140 (named follow-up), ADR-003/005/007.
