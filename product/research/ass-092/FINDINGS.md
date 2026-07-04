# FINDINGS: Contradiction Detection Restoration — NLI as-was

**Spike**: ass-092 · **Approach**: investigation (read-only code + removal-commit archaeology) · **Confidence**: directional (every claim grounded in working-tree `file:line` and the actual removal commits; no PoC required). **Tracking**: GH #899 · **Capability**: SL-CONTRADICT (regressed).

## Executive orientation

The NLI **substrate never left**. What was torn out is a **single link**: the automatic **Contradicts-edge WRITE**. Everything upstream (NLI cross-encoder load/invoke, the `nli_enabled` gate, the per-pair contradiction score) is **present and live**; everything downstream that consumes Contradicts edges (col-030 serve-time suppression) is **present, functional, and starved**. In the current tick the contradiction score is computed and then **discarded** — `nli_detection_tick.rs:718` *"Contradiction is discarded (C-13 / AC-10a)."*

Restoration is small and low-risk: re-instate the discarded write inside the already-`nli_enabled`-gated Path B. No `unimatrix-embed` change (avoids the 40–90 s ort rebuild penalty), no schema/migration, and no reversal of NLI-from-ranking (separately enforced by `w_nli = 0.0`, untouched).

## Q1 — Current-state inventory: REMOVED / DISABLED / STARVED

Removal chain: **crt-023** (#327/#328) added the substrate incl. `run_post_store_nli` which wrote **both Supports and Contradicts**; **crt-029** (#412) added the Supports-only tick; **crt-038** (#483, `420704b3`) **deleted** `run_post_store_nli`, bootstrap promotion, and the `nli_auto_quarantine_allowed`/`parse_nli_contradiction_from_metadata` guard, and added the `w_nli==0.0` short-circuit — **this deleted the automatic Contradicts write**; **crt-039** (`605f985c`) removed the `nli_enabled` gate from `background.rs`; **crt-040** (`a544611d`) put cosine Supports (Path C) in the tick; **docs `a870d073`** scrubbed NLI from `README.md`/`config.toml` (docs only — code config fields survive).

| Piece | Status | Evidence (`file:line`) |
|---|---|---|
| NLI substrate — `NliProvider`,`CrossEncoderProvider`,`NliModel`,`ensure_nli_model`,`NliScores`,`softmax_3class`,`truncate_input` | **PRESENT + LIVE** | `unimatrix-embed/src/cross_encoder.rs:35,49,71,165,188`; `model.rs:84`; `download.rs:76`; `lib.rs:18,20,30`; non-test callers in `nli_handle.rs`, `search.rs:425`, `nli_detection_tick.rs:56` |
| ONNX runtime | **PRESENT** | `ort = "=2.0.0-rc.9"` `unimatrix-embed/Cargo.toml:12`; session `cross_encoder.rs:120`; invoke `score_batch:235` |
| `NliServiceHandle` + (expected) SHA-256 gate | **DISABLED** | `infra/nli_handle.rs:125` — `if !config.nli_enabled` → `NliNotReady`; provider never loads |
| **`nli_enabled` config attribute (anchor)** | **PRESENT + WIRED** (not dangling) | `config.rs:526` default `false` (`default_nli_enabled:1023`); gates model load `nli_handle.rs:125`, pool floor/boot `main.rs:856,876,1578`, tick Path B `nli_detection_tick.rs:566`, per-slug `http_provision.rs:150,253` |
| NLI scoring in tick (Path B) | **DISABLED + output-discarded** | `nli_detection_tick.rs:566` gate, `:638-647` `score_batch`, `:718` contradiction discarded |
| **Automatic Contradicts WRITE (NLI)** | **REMOVED** | `run_post_store_nli` deleted crt-038; tick doc `:157` "Never writes Contradicts"; no `"Contradicts"` literal in tick |
| Manual Contradicts WRITE (`context_edge`/`context_correct`) | **PRESENT** | `mcp/edge_write.rs:200-221` (source=agent) |
| `write_nli_edge`/`write_graph_edge` helpers | **PRESENT** (write any relation_type) | `services/nli_detection.rs:25,81` |
| crt-003 detector `scan_contradictions` (cosine ≥0.85 + heuristic) | **PRESENT + FUNCTIONAL** | `infra/contradiction.rs:161` (threshold `:21`, weights `:33-39`); called `background.rs:681,1088`; returns pairs — **counts only, writes no edge, quarantines nothing** |
| Auto-quarantine (crt-003 lineage) | **PRESENT, NOT contradiction-linked** | `background.rs:1406`; effectiveness-driven (`Ineffective`/`Noisy` × cycles, `:1232-1252`) |
| NLI auto-quarantine **guard** (crt-023 ADR-007) | **REMOVED, now moot** | deleted crt-038; `query_contradicts_edges_for_entry` (`read.rs:1546`) has zero production callers |
| Serve-time suppression (col-030 `suppress_contradicts`) | **PRESENT + FUNCTIONAL, STARVED** | `engine/graph_suppression.rs:44`; consumed `search.rs:1381` (Step 10b) |
| Contradiction-density in Lambda (crt-051) | **PRESENT + FUNCTIONAL, FED (not starved)** | `coherence.rs:78`; fed by `scan_contradictions` pair count `status.rs:611`, not by NLI edges |
| NLI search re-ranking (crt-023) | **PRESENT but INERT for ranking** — DO NOT TOUCH | `search.rs:1214` `try_nli_rerank` still called; `effective():161` short-circuits at `w_nli==0.0`; `default_w_nli()=0.00` `config.rs:1055` |

**Orphaned config** (validated/merged, no reader): `nli_contradiction_threshold` (`:571`), `max_contradicts_per_tick` (`:580`), `nli_auto_quarantine_threshold` (`:589`).

**Sharp line:** REMOVED (rebuild) = the automatic NLI Contradicts write. DISABLED (re-enable) = `nli_enabled` gate + Path B scoring behind it (already intact). STARVED (auto-reconnects on write) = col-030 `suppress_contradicts`. NOT affected = density and quarantine (fed by other inputs; NLI Contradicts edges feed neither in current code).

## Q2 — Restoration scope

**Recommendation (minimal-modernization — restore at the surviving home):** re-instate the Contradicts write inside `run_graph_inference_tick` Path B where the score is discarded.

- **Rebuild (the one removed link):** in `nli_detection_tick.rs` Phase-8 write loop (~`:700-730`), alongside the Supports write, add `if nli_scores.contradiction > config.nli_contradiction_threshold { write_nli_edge(store, source_id, target_id, "Contradicts", nli_scores.contradiction, timestamp, &metadata_json) }`, budget-capped. Every part already exists (score, threshold, writer `write_nli_edge`, `format_nli_metadata`). ~one branch + a counter.
- **Re-enable:** nothing to re-wire — `nli_enabled` already admits Path B; the new code lives inside that gate.
- **Reconnect:** automatic — `suppress_contradicts` (`search.rs:1381`) consumes new Contradicts edges at serve time; no col-030 change.
- **Do NOT restore the auto-quarantine guard.** Contradicts edges are excluded from graph traversal/PPR (`graph_expand.rs:68,128`; `graph_ppr.rs:188`) and unused by `effectiveness/` (zero hits), so the guard would be dead code. **FLAG (divergence from literal as-was):** the "Contradicts → quarantine" linkage is gone at the consumer end; restoring the writer does not reconnect quarantine — faithful to current architecture.

**Config-gate confirmation:**
- `nli_enabled=true`: provider loads (`nli_handle.rs:125`) → Path B runs (`:566`) → contradiction computed → Contradicts edge written → suppression consumes it. Full path.
- `nli_enabled=false` (SDLC default): `get_provider()` → `NliNotReady`; Path B returns at `:566` before any NLI call. Because the new code sits entirely inside the gated region, the disabled path is **byte-identical to today**.

**Per-slug (vnc-040):** `nli_enabled` already flows through the overlay (`http_provision.rs:150,253`, "NEVER hardcoded false"); the per-slug tick reads it (`jobs.rs:345`). A per-domain toggle composes with no extra work.

**Graceful degradation (Principle 5):** absent/failed/hash-mismatched model → `NliServiceHandle::Failed` → `get_provider()` Err → Path B returns at `:576` before any write. Identical to disabled path — never broken.

**Two deployment modes — confirmed in BOTH:** (1) single-project/local daemon: `spawn_background_tick` → `background.rs:754` calls `run_graph_inference_tick`; (2) multi-project HTTP/per-slug: per-slug loop → `background/jobs.rs:345` calls the same function with the slug's config. Both call the same function, so the write fires in both, each reading its own `nli_enabled`/`nli_contradiction_threshold`. No mode-specific code needed.

**All elements to be effective:** detect (Path B, un-discard) → write (`write_nli_edge("Contradicts")`) → serve-time suppression (col-030, live). That is the complete effective chain; density and quarantine need no work and should not be forced back in.

**Schema/migration: NONE.** `relation_type` is free-text (no `CHECK IN`); `migration.rs:462-464` explicitly says "All Contradicts edges are created at runtime by W1-2 NLI." `write_nli_edge` writes only existing columns.

**Blast radius:** Lambda/coherence unaffected (density uses `scan_contradictions`; Contradicts excluded from traversal/PPR); auto-quarantine unaffected (excludes Contradicts); enrichment tick gains one budget-capped branch; search already runs `suppress_contradicts` every query.

**Observable-behavior flags:** (1) **Timing/candidate set** — as-was wrote Contradicts immediately post-store over the new entry's HNSW top-k (`nli_post_store_k`, budget `max_contradicts_per_tick`); Path B writes on the background tick over cosine-floor-gated candidate pairs (budget `max_graph_inference_per_tick`). Edges appear next tick, not instantly; candidate population differs. (2) **Search re-rank compute** — enabling `nli_enabled=true` also re-activates `try_nli_rerank` NLI invocation (`search.rs:1214`), inert for ranking (`w_nli=0.0`, decoupling NOT reversed) but adds per-query latency; if unwanted, consider a separate gate. (3) Quarantine linkage not restored (above).

**Rejected alternative (literal as-was, Option A):** revert crt-038 to resurrect `run_post_store_nli` + bootstrap promotion + `NliStoreConfig` (~1,200 lines across `nli_detection.rs`, `store_ops.rs`, `background.rs`, `services/mod.rs`). Matches original timing but duplicates the NLI scoring the tick already does and adds test surface for no behavioral gain over Option B. Not recommended.

**Effort + risk:** SMALL — primary change is one production file (`nli_detection_tick.rs` Path B) + un-orphaning `nli_contradiction_threshold` in `config.rs`; optional doc restore of `config.toml`/`README.md` (reverse `a870d073`) if operator-discoverable. No `unimatrix-embed` change (sidesteps the crt-023 256-cycle / 40–90 s ort rebuild pain); pure logic testable via already-extracted `softmax_3class` (`cross_encoder.rs:188`), `truncate_input:165`, `format_nli_metadata` (`nli_detection.rs:126`) with zero model load; use `cargo test -p unimatrix-server -- <name>`, never `--workspace`. Risk LOW — new code confined to an already-gated region, disabled path provably untouched, no migration, downstream consumer already tested.

## Q3 — Generalized small-ONNX substrate (assessment)

**Feasibility HIGH.** The repo already runs two ONNX models over the same substrate as separate structs: `OnnxProvider` (embedding, `onnx.rs:21`) and `NliProvider` (cross-encoder, `cross_encoder.rs:71`) — both `Mutex<Session>` via `Session::builder()…commit_from_file`, both `tokenizers`, near-duplicate downloaders (`ensure_model` `download.rs:11` vs `ensure_nli_model:76`). Generalization is a refactor of existing duplication, not new capability.

**Shape:** a generic `OnnxSession` wrapper; a `ModelDescriptor` trait (cache_subdir/filename/repo, input tensor names — embedding uses 3 incl. `token_type_ids`, NLI 2 — and output post-processor: mean-pool+L2 vs `softmax_3class`); a generic `ensure_model(descriptor)`; a generic `ModelHandle` state machine generalized from `NliServiceHandle`, **with SHA-256 verification moved in-crate** (today `download.rs` verifies only existence/size; the hash gate is only *expected* to exist, `cross_encoder.rs:90-92`). NLI and embedding each become a `ModelKind`; future small models plug in via the trait.

**Effort/risk delta: MODERATE-to-LARGE additional, and it lands in `unimatrix-embed`** — so it pays the ort rebuild penalty that Option B avoids. Separate, larger refactor. **Does the as-was restoration foreclose it? NO** — Option B touches only the server tick + config, leaving `unimatrix-embed` free to be generalized later. **Recommendation:** keep separate — ship the small restoration now; treat the ONNX-substrate generalization as its own future feature that re-homes NLI + embedding onto a shared path without changing observable contradiction behavior, and closes the in-crate SHA-256 gap.

## Unanswered Questions

None blocked. Two items are **deliberately deferred by SCOPE** (not spike failures): research-domain **value** of contradiction detection, and **SLN1 poisoning-defense** contribution — both await a mature corpus and belong to a future value-analysis spike. **No value verdict is given, by design.**

## Out-of-Scope Discoveries (carry-forwards)

1. **Orphaned NLI config trio** (`nli_contradiction_threshold`, `max_contradicts_per_tick`, `nli_auto_quarantine_threshold`, `config.rs:571,580,589`) — restoration un-orphans the first; decide re-use vs removal for the others. Dead validated config invites false "it's wired" assumptions.
2. **`query_contradicts_edges_for_entry` dead in production** (`read.rs:1546`) — if the guard stays removed, deprecate this store method.
3. **SHA-256 model-integrity gate not implemented in `unimatrix-embed`** (only existence/size checked; `download.rs`) — SLN1 defense depends on it; natural home is the Q3 substrate.
4. **`nli_enabled` couples two capabilities** (search re-rank + contradiction detection) — a future split gives per-domain cost control (ties to vnc-040).

## Recommendations Summary

- **Q1:** Substrate, `nli_enabled` gate, Path B scoring, col-030 suppression, and density are all present; only the **automatic Contradicts WRITE is REMOVED**. col-030 is the one STARVED consumer; density/quarantine feed off other inputs.
- **Q2:** Un-discard the contradiction score in `run_graph_inference_tick` Path B and write a `Contradicts` edge via `write_nli_edge` when `contradiction > nli_contradiction_threshold`, budget-capped — one branch inside the existing `nli_enabled` gate. No schema, no embed change, no ranking reversal. Do **not** restore the auto-quarantine guard. Effort SMALL, risk LOW.
- **Config gate + dual mode:** `enabled=true` drives detect→write→suppress; `enabled=false` is byte-identical to today; both single-project (`background.rs:754`) and per-slug (`jobs.rs:345`) ticks call the same function → works in both deployment modes and composes with the vnc-040 per-slug overlay for a per-domain toggle.
- **Q3:** Feasible and **not foreclosed**; keep as a separate future `unimatrix-embed` refactor. Ship restoration standalone first.
- **Regression tests:** (1) enabled + contradiction>threshold → Contradicts edge (source='nli') written; (2) disabled → zero Contradicts written (byte-identical); (3) strict-`>` boundary; (4) provider Err / model absent → no write, no panic; (5) per-tick budget cap; (6) integration: written Contradicts edge consumed by `suppress_contradicts` (reuse col-030 `T-SC` fixtures, `search.rs:4143`); (7) density unchanged by NLI Contradicts edges; (8) both tick call-sites honor per-config `nli_enabled`. Pure-function units (`softmax_3class`, `truncate_input`, `format_nli_metadata`) run with zero ONNX via `cargo test -p unimatrix-server -- <name>`.
- **No value verdict** — restoration readiness only, per SCOPE.
