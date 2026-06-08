## ADR-005: One Distill Helper at All Four Success Returns, Gated on an Exhaustive `TranscriptRetention` Match

### Context
Pattern #4750 (SR-05): `context_cycle_review` has **four** `result.is_ok()` success returns
(`tools.rs:2110` purged-signals, `:2236` cached-MetricVector, `:2925` memoization-hit, `:3027`
full-pipeline), each of which already fires `purge_cycle_transcripts`. Inserting distillation at only
the tail return silently skips the cache-hit and degraded paths — invisible in happy-path tests
(AC-05). The distill+purge behavior is an enterprise seam: it must be gated on an **exhaustive**
`TranscriptRetention` match (vnc-024 ADR-005 #4721, AC-10) — `PurgeOnCycleClose` (`server.rs:543`) is
the only OSS-honored arm; `RetainDays(_)` (`:551`) is unreachable (rejected at `validate()`) and must
neither distill nor purge. The 500-line rule (Constraint 10) forbids growing `tools.rs`.

### Decision
One shared helper, in a new thin module `unimatrix-server/src/mcp/distill_handler.rs`, called at all
four sites immediately **before** the existing `purge_cycle_transcripts` call, with the same
`result.is_ok()` gating:

```rust
fn distill_before_purge(
    registry: &SessionRegistry,
    feature_cycle: &str,
    observations: &[ObservationRecord],   // already loaded by load_cycle_observations
    cfg: &RetentionConfig,
) -> Option<TranscriptCandidatesSection>
```

The helper, in order:
1. **Exhaustive gate** — `match cfg.transcript_retention { PurgeOnCycleClose => proceed, RetainDays(_)
   => return None }` (no wildcard arm; the match is the enterprise seam, AC-10). `RetainDays` is
   unreachable in OSS, so this arm is structurally dead but written explicitly.
2. `registry.take_transcripts_for_feature(feature_cycle)` (ADR-001) — snapshot off-lock.
3. For each snapshot: empty/hole-ridden past threshold (ADR-006) → reconstruct; else → `select_candidates`
   (ADR-003).
4. Enforce the **per-cycle aggregate cap** (`transcript_candidate_cycle_cap_bytes`, config knob, OQ-3)
   across the union; attach per-session `SessionLossInfo` (ADR-007).
5. Return `Some(section)` if any candidate, else `None`.

The handler attaches the returned `Option` at response-assembly level (ADR-004), then calls
`purge_cycle_transcripts` as today — distill strictly precedes purge at every site (AC-05). Error paths
are untouched: they keep transcripts and produce no candidates (existing behavior).

An **exhaustiveness regression test** asserts distillation fires at all four returns and would fail if a
fifth success return is added without wiring it (SR-05) — modeled on the same shape vnc-025 used for the
purge.

### Consequences
Easier: one helper means the four sites stay in lockstep (purge and distill share the same gating and
ordering); the exhaustive enum match makes `RetainDays` neither-distill-nor-purge true by construction
(AC-10); `tools.rs` gets four thin call lines, all real logic in `distill_handler.rs` + the observe
module (Constraint 10). Harder: four call sites must each be edited and tested (the regression test is
the guard against drift); the helper bridges server registry types and the pure observe module, so its
signature must thread both the registry and the already-loaded observations without re-querying.
Cross-refs: ADR-001 (snapshot), ADR-003 (selection), ADR-004 (attach), ADR-006 (reconstruction +
trigger), ADR-007 (loss info), pattern #4750, vnc-024 ADR-005 #4721, vnc-025 ADR-004 #4742.
