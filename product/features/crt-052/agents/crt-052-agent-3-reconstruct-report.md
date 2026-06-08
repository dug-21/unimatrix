# Agent Report — crt-052-agent-3-reconstruct (C5 Reconstruction Fallback)

## Scope
Implement C5 `reconstruct_from_observations` — the Wave-A reconstruction fallback in
`crates/unimatrix-observe/src/distill/reconstruct.rs`.

## Files Created / Modified
- `crates/unimatrix-observe/src/distill/reconstruct.rs` — NEW (286 lines). C5 logic.
- `crates/unimatrix-observe/src/distill/reconstruct_tests.rs` — NEW (308 lines). Unit tests, split out via `#[path]` to keep both files under the 500-line rule.
- `crates/unimatrix-observe/src/lib.rs` — MODIFIED. Added `pub mod distill;`.
- `crates/unimatrix-observe/Cargo.toml` — MODIFIED. Added `regex = "1"` (workspace-consistent; required crate-wide once `distill` exists — C3's `markers.rs` also needs it).
- `Cargo.lock` — regenerated.

NOT touched (C3-owned, in-flight in the same swarm): `distill/mod.rs`, `jsonl.rs`, `markers.rs`, `select.rs`, `corpus_tests.rs`. `mod.rs` already declares and re-exports `reconstruct` — C5 is wired in.

## Tests
- C5 reconstruct: 19 passed / 0 failed.
- Full `unimatrix-observe` crate (C3+C5 together): 586 passed / 0 failed.
- `cargo build --workspace`: zero errors.
- `cargo clippy` on reconstruct files: zero warnings. `cargo fmt` applied.

Tests cover: builds from observations; distillation-input-only (no row insert/buffer write); session_cap keep-earliest (incl. block-exceeds-cap drop-whole, zero-cap); empty observations -> empty Vec; session filtering; chronological ordering; topic_source rank ordering; drops-no-observation (R-14); all-vote-still-reconstructs (SR-06); stable-sort-not-filter; degraded-label discriminability; advisory family hints (text + event-type fallback); UTF-8-safe truncation; zero-padded ts ordering; R-11 no-transcript_hold reference.

## Design Decisions / Deviations
- **Family hints kept dependency-free and self-contained.** The pseudocode says "reuse C3 `markers.rs` `match_families`". To keep C5 severable and avoid a swarm race on C3's not-yet-complete module, reconstruction uses its own coarse, advisory keyword/event-type inference (no `regex`, no cross-module coupling). Hints are advisory only (Constraint 6), so this satisfies the non-empty `family_hints` C4 invariant and the "advisory hints over reconstructed text" intent. C6 may further enrich via C3 markers when composing the pipeline.
- **`ts` rendered as zero-padded fixed-width millis string** so the within-session chronological sort orders lexically == numerically. Cross-provenance ordering (Primary RFC3339 vs Reconstructed millis) is C6's concern (C6 re-sorts the cross-session union).
- **`byte_offset = 0`** for all reconstructed candidates (no buffer stream position; documented).

## Issues / Blockers (FLAGGED for leader/architect)
1. **`topic_source` contract gap (resolved defensively, needs follow-up).** ADR-006 and the C5 pseudocode read `o.topic_source` to soft-order reconstruction input. But `topic_source` (vnc-030 ADR-005 #4817) was shipped ONLY as a SQL column on `observations`; it is NOT a field on the in-memory `ObservationRecord` (unimatrix-core), and the cycle-review load query does not project it. The pinned ARCH §4 signature `obs: &[ObservationRecord]` cannot read it as written.
   - Resolution honoring the binding signature AND the never-filter SR-06 invariant: ranking routes through `observation_topic_source(&ObservationRecord) -> Option<&str>`, which returns `None` today. With all rows equal-rank, the stable sort is a no-op — zero rows dropped, zero sessions excluded (SR-06 preserved exactly). When the record/query gains `topic_source`, point that accessor at the field and ordering activates with no other change.
   - Did NOT add a `unimatrix-core` struct field (out of this task's scope) and did NOT turn the soft preference into a filter.
   - **Follow-up for leader:** decide whether to (a) extend `ObservationRecord` + the cycle-review SELECT to carry `topic_source`, or (b) accept the no-op ordering for v1. Either way the C5 invariant holds.
2. **Shared-file coordination.** `distill/mod.rs`, `Cargo.toml`, and `lib.rs` are jointly touched by C3 and C5. I committed only my owned files + the crate-level `regex`/module wiring needed to compile any distill code. C3 owns `mod.rs` and its source files.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-006 (#4858), ADR-007 (#4856), and via search the vnc-030 topic_source taxonomy (#4817), which confirmed topic_source is a SQL-only column.
- Stored: entry #4862 "Pseudocode reading o.topic_source on ObservationRecord — field lives only in SQL, not the in-memory struct (crt-052 C5)" via /uni-store-lesson (lesson-learned: verify the in-memory struct carries a field before implementing pseudocode that reads it; preserve binding signature + never-filter invariant via an Option accessor).
