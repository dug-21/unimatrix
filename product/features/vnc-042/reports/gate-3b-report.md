# Gate 3b Report: vnc-042

> Gate: 3b (Code Review)
> Date: 2026-07-01
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | Handler + formatter match pseudocode; only departure is a sound refactor (inline match → `resolve_effective_id`/`finalize_note` free fns) that improves testability, same logic |
| 2. Architecture compliance | PASS | 3-component decomposition intact; ADR-001/002/003 all faithfully implemented; canonical `follow_to_current` bound via fully-qualified path |
| 3. Interface implementation | PASS | `format_single_entry_with_note`, `ResolutionNote`, `follow_to_current` re-export all match Integration Surface; `effective_id` threaded to BOTH fetch and edges |
| 4. Test case alignment | PASS | R-01..R-12 covered; behavioral default-on test is truly behavioral; canary + shape tests unedited and green |
| 5. Code quality | PASS (1 WARN) | Build clean, clippy `-D warnings` clean, no stubs/TODO, no non-test `.unwrap()`. WARN: file lengths (entries.rs 937, tools.rs 12749) exceed 500 — pre-existing, architecture-approved edit targets |
| 6. Security | PASS | No new deps (CVE surface unchanged); `follow_supersessions` plain `Option<bool>` rejects quoted scalar; DoS bounded by untouched 50-hop cap; SQL parameterized; no secrets |
| 7. Knowledge stewardship | PASS (1 WARN) | Both agent-3 (rust-dev) reports carry compliant blocks with `Queried:` + `Stored:`. WARN: formatter agent's `/uni-store-pattern` failed (anonymous-agent Write capability) — content captured for leader/retro |

## Detailed Findings

### 1. Pseudocode fidelity
**Status**: PASS
**Evidence**: `tools.rs` handler follows the pseudocode control flow exactly: `validated_id` (no cast) → resolve → single fetch on `effective_id` → finalize note → edges on `effective_id` → format route → audit. The branch truth table in `context-get-handler.md` is realized 1:1 in `resolve_effective_id` (`tools.rs:82-97`) + `finalize_note` (`tools.rs:105-124`). formatter (`entries.rs`) matches `response-formatter.md` note-placement tables (prefix for Followed/DeadEnd, suffix for AsStored; json `resolution` object).
**Departure (documented, benign)**: pseudocode showed an inline `match` in the handler body; the implementation extracts the same logic into two pure free functions (`resolve_effective_id`, `finalize_note`). This is an improvement, not a drift — it makes the resolution seam unit-testable without a `RequestContext` (which is not constructible in unit scope) and preserves the exact branch semantics. Verified by 19 passing seam tests.

### 2. Architecture compliance
**Status**: PASS
**Evidence**:
- **ADR-001** (handler-owned default): `follow_supersessions: Option<bool>` with `#[serde(default)]` (`tools.rs:278-288`), NOT bare `bool`. Default owned by handler branch `None | Some(true) => follow` (`tools.rs:91`). Confirmed the load-bearing invariant.
- **ADR-002** (dead-end fail-loud): resolution calls `crate::mcp::graph_read::follow_to_current` (`tools.rs:91`) — the canonical copy, NOT `handle_current`, NOT the `graph_read_supersession.rs:122` duplicate. `None => (id, DeadEnd{requested: id})` returns the originally-requested id with a loud flag.
- **ADR-003** (response construction): note attaches only via `format_single_entry_with_note`; clean path routes to untouched `format_single_entry`; `resolution` json key present only on non-clean paths; edges rebuilt on `effective_id`.
- Visibility widen `pub(super)`→`pub(crate)` (`graph_read_neighbors.rs:36`) + re-export (`graph_read.rs`) match Component-3 plan; body unchanged.

### 3. Interface implementation
**Status**: PASS
**Evidence**: `format_single_entry_with_note(&EntryRecord, ResponseFormat, Option<&EdgesView>, &ResolutionNote) -> CallToolResult` matches the Integration Surface signature. `ResolutionNote` enum defined in `entries.rs` and re-exported through `response/mod.rs`. `effective_id` threaded to BOTH `entry_store.get(effective_id)` (`tools.rs:156`) AND `build_edges_view(&self.store, effective_id)` (`tools.rs:179`) — the R-03 single-fetch invariant. `follow_supersessions` is plain `Option<bool>`, no `deserialize_*_or_string` coercion (NFR-02).

### 4. Test case alignment
**Status**: PASS
**Evidence** (all green — see Verification):
- **R-01/TS-01/TS-02**: byte-identity canary `test_none_json_byte_identical_to_base_object` unedited and green; `format_single_entry` and the ~15 shape tests untouched (entries.rs diff is pure-addition, zero deletions; response/mod.rs diff only touches the re-export block). Strip-and-compare `test_with_note_stripped_equals_base_formatter` proves additivity across all 3 formats.
- **R-02 (Critical)**: `test_get_handler_field_absent_resolves_to_terminal` is genuinely behavioral — it deserializes JSON that OMITS the field, asserts `None`, then drives `resolve_effective_id` and asserts it resolves to terminal B (`PreNote::Followed{a,b}`). Not a serde round-trip; a bare-`bool` footgun would fail this. Plus explicit true/false/absent three-state and quoted-scalar-rejection tests.
- **R-03**: `test_get_handler_resolved_edges_keyed_on_terminal`, `test_get_handler_effective_id_independent_of_format_and_edges`.
- **R-04**: orphaned, quarantined, >50-hop, self-cycle, and store-error all yield `DeadEnd{requested}` with returned id == requested (5 tests).
- **R-06**: `test_with_note_body_matches_base_across_formats`.
- **R-07**: presence/absence of `resolution` key across all four ADR-003 cases.
- **R-08**: `test_note_asstored_null_successor_wellformed_footer` asserts no `#null`, no `superseded by #`, no panic; json `superseded_by: null`.
- **R-09**: `test_get_tool_description_documents_follow_supersessions` (proxy).
- **R-05/R-10/R-11/R-12**: build/clippy green (R-05); R-10/R-12 accepted-documented; R-11 (JS parity) correctly budgeted as a post-PR CI round-trip, not code.
**Note**: end-to-end handler-route/format proofs (full `context_get` through `RequestContext`) are correctly deferred to the Stage 3c integration suite; the seam + formatter unit coverage is the appropriate 3b scope.

### 5. Code quality
**Status**: PASS (1 WARN)
**Evidence**: `cargo build -p unimatrix-server` clean; `cargo clippy -p unimatrix-server --lib -- -D warnings` clean; no `todo!()`/`unimplemented!()`/`TODO`/`FIXME`; no `.unwrap()` in non-test code (production paths use `.map_err`/`.unwrap_or_default()` matching existing formatter idiom). All `.unwrap()` occurrences are inside `#[cfg(test)]` modules.
**WARN (not a rework blocker)**: `entries.rs` is now 937 lines and `tools.rs` 12749 — both exceed the 500-line guideline. These are pre-existing structural conditions: `tools.rs` is a long-standing monolith the approved ARCHITECTURE explicitly names as the edit target, and `entries.rs` co-locates its `#[cfg(test)]` module per Rust convention (production additions are ~149 non-test lines). Flagging these as FAIL would contradict the architecture-approved edit surface. Recorded as debt, not blocking.

### 6. Security
**Status**: PASS
**Evidence**: No Cargo.toml / Cargo.lock changes → CVE surface unchanged from baseline (cargo audit unaffected by this diff). Two untrusted inputs: `id` (unchanged `validated_id`) and `follow_supersessions` (plain `Option<bool>`; `test_get_params_follow_supersessions_no_quoted_scalar_coercion` proves a quoted scalar errors rather than coercing — closes the #3728 class). DoS bounded by the untouched 50-hop cap. No new by-id reachability (resolution only changes which already-authorized entry is returned). Test SQL is parameterized (`bind`). No hardcoded secrets.

### 7. Knowledge stewardship compliance
**Status**: PASS (1 WARN)
**Evidence**: Both implementation (agent-3) reports carry a compliant `## Knowledge Stewardship` block:
- `vnc-042-agent-3-context-get-handler-report.md`: `Queried:` (#3538, #317, #3728, applied plain `Option<bool>` no-coercion) + `Stored:` entry #5389 (pattern: unit-testing rmcp `#[tool]` handler logic via extracted `pub(crate)` seam fns) — matches the observed `resolve_effective_id`/`finalize_note` refactor.
- `vnc-042-agent-3-response-formatter-report.md`: `Queried:` (ADR-004 #87, #3459/#298, `format_store_success_with_note` precedent) + `Stored:` documented.
**WARN (non-blocking)**: the formatter agent's `/uni-store-pattern` attempt was rejected ("Agent 'anonymous' lacks Write capability"); the block is present with a reason and the content was captured for the leader/retro to store under `unimatrix-server`. This is an infra/capability limitation, not a stewardship omission — flagged to the coordinator so the pattern is not lost.

## Flagged-Item Decisions

### AUDIT-ID CHOICE — ACCEPT
`target_ids`, `record_access`, and `record_confirmed_entry` now record `effective_id` (`tools.rs:216,229,242`); the audit `detail` names both ids when they differ (`retrieved entry #{effective_id} (requested #{id})`, `tools.rs:204-208`). Sound audit semantics: access-accounting and usage signals should attach to the entry actually returned to the caller (the effective entry), and confirmed-entry genuinely is the returned entry. On DeadEnd/AsStored/Clean paths `effective_id == id` (no behavior change); only on Followed do they differ, where recording the terminal is correct and the requested id is preserved in the human-readable detail for traceability. Gate 3a's deferred WARN is resolved as accepted.

### validation.rs one-line struct-literal fill — ACCEPT
`follow_supersessions: None` added to the sole external `GetParams` construction site (`validation.rs:1030`, a `#[cfg(test)]` block asserting `validate_get_params` errors). Benign mechanical closed-set ripple: `validate_get_params` does not inspect the new field, so the assertion is unchanged; no masked behavior change. Completeness verified — grep finds only two `GetParams {` sites total (the definition + this one), and `cargo build` passing confirms no construction site was missed.

## Verification Commands (truncated per protocol)

- `cargo build -p unimatrix-server` → Finished, clean
- `cargo clippy -p unimatrix-server --lib -- -D warnings` → Finished, clean
- `cargo test -p unimatrix-server --lib get_resolution_tests` → 19 passed / 0 failed
- `cargo test -p unimatrix-server --lib response::entries::tests` → 13 passed / 0 failed
- `cargo test -p unimatrix-server --lib mcp::response::tests` → 83 passed / 0 failed (incl. byte-identity canary + shape tests)
- `git diff --stat …Cargo.toml Cargo.lock` → empty (no dependency changes)

## Rework Required

None for code correctness. One coordinator follow-up: confirm the implementation-agent (`uni-rust-dev`) reports carry a compliant `## Knowledge Stewardship` block (Check 7 could not be evaluated from the feature directory).
