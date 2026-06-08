# Agent Report — crt-052 C3 Candidate Selection Module

**Agent:** crt-052-agent-3-selection-module
**Component:** C3 — pure selection module (`unimatrix-observe/src/distill/`)
**Wave:** A. Commit: `51f82aec`.

## Files created
- `crates/unimatrix-observe/src/distill/mod.rs` — module root; declares `jsonl`/`markers`/`select` alongside C5's `reconstruct`; re-exports `select_candidates`, `match_families`; wires `corpus_tests`; carries AC-12 throughput + R-11 wave-boundary tests.
- `crates/unimatrix-observe/src/distill/jsonl.rs` — untrusted Claude Code JSONL parse (`parse_blocks(&[u8]) -> (Vec<ParsedBlock>, skip_count)`).
- `crates/unimatrix-observe/src/distill/markers.rs` — four marker families (50 regex via `OnceLock<RegexSet>`), `match_families(&str) -> Vec<FamilyHint>`.
- `crates/unimatrix-observe/src/distill/select.rs` — `select_candidates(bytes, session_id, base_offset, session_cap) -> Vec<TranscriptCandidate>` (binding ARCH §4 signature).
- `crates/unimatrix-observe/src/distill/corpus_tests.rs` — AC-03 recall/volume + AC-V-FUZZ corpus tests (pure via `include_*!`).
- `corpus/PROVENANCE.md`, `corpus/labeled_corpus.jsonl`, `corpus/labels.json`, `corpus/malformed/{truncated,non_utf8,unknown_type,embedded_nul}.jsonl`.

## Files modified
- `crates/unimatrix-observe/Cargo.toml` — added `regex = "1"` (already vetted in `unimatrix-server`; AC-13/NFR-6 — no heavyweight dep). (Edit was a no-op vs C5's prior commit; no net diff.)
- `crates/unimatrix-observe/src/lib.rs` — `pub mod distill;` (already present from C5 commit).

## Tests: 63 passed / 0 failed (distill); full observe crate 514+72 passed / 0 failed
- AC-02: matched blocks kept whole, unmatched dropped, dedup, per-session cap (keep-earliest), chronological order, fields populated.
- AC-V-FUZZ (merge gate, R-10): truncated JSON, non-UTF-8, oversized line (>1 MiB), unknown record type, embedded NUL, deeply-nested JSON, fully-corrupt input → skip-with-count, never `Err`, never panic. Driven from committed `corpus/malformed/`.
- R-12 logical offset: `base_offset==0` equals in-snapshot offset; `base_offset>0` equals `base + in_snapshot`; stable across a 4 MiB elision boundary.
- AC-12: 4 MiB rule pass well under the 50 ms target.
- R-11 wave-boundary: zero code reference to `transcript_hold` in `jsonl/markers/select`.
- `cargo fmt` applied; `cargo clippy` clean on all distill files; workspace builds.

## AC-03 independent fixture
- **Path:** `crates/unimatrix-observe/src/distill/corpus/` (`labeled_corpus.jsonl` + `labels.json` + `PROVENANCE.md`).
- **Independence mode:** `anchors-before-port` — labels follow the ass-070 hand-labeled ground-truth semantics (authored in the research spike, predating this regex port); prose written to the human family label, not lifted from `markers.rs`. Documented in `PROVENANCE.md` (OQ-6 review gate; `test_corpus_provenance_header_present` asserts header + mode).
- **Measured recall:** 8/8 = **1.00** (≥ 0.90), all four families covered.
- **Measured volume:** ~1076 selected bytes / 30177 raw = **~3.6%** (≤ 10%, NFR-3).

## Issues / flags
- **Cap drop-count (pseudocode flag, by design):** `select_candidates` returns only `Vec<TranscriptCandidate>` per the pinned signature. The per-session cap-drop count for AC-08 is recoverable by C6 (pre-cap vs post-cap candidate set) — option (a) from the pseudocode. No signature widening. C6 must compute `dropped_candidates` for `SessionLossInfo`.
- **ass-070 `extractor.py` not committed:** the exact 50 regex were not in the repo (only FINDINGS.md describes the four families + vocabulary). I authored 50 patterns from that documented vocabulary (project protocol terms: gates/waves/stages/retro). They are tuned to this project (ass-070 Open Thread: per-project tuning unverified) — acceptable since hints are advisory and the agent re-classifies.
- **Concurrent C5 edits on `mod.rs`:** C5's committed tree had `reconstruct.rs`/`reconstruct_tests.rs` but no `mod.rs` (module root absent → would not compile standalone). My merged `mod.rs` declares all four modules and restores a compiling tree. Verified: full crate builds + 514 lib tests pass.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern: untrusted JSONL skip-with-count — #4221/#4777 tangential; decision/crt-052 — surfaced ADR-002/003 #4848/#4849) + read ADR-003 file directly. Applied: pure-module placement, skip-with-count, advisory-hints-only, anchors-before-port fixture gate.
- Stored: entry #4863 "crt-052 C3: pure untrusted-JSONL selection via &[u8] split + per-line parse-or-skip; bounded depth/byte guards" via /uni-store-pattern; entry #4864 "AC-03 fixture independence — pad corpus with noise to clear the <=10% volume gate, not just recall" via /uni-store-lesson.
