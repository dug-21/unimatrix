# Test Plan Overview — vnc-040: Per-Slug Configuration Overlay Resolution (C6 / Feature A)

> Rooted in RISK-TEST-STRATEGY.md (14 risks / 32 scenarios) and ACCEPTANCE-MAP.md (AC-01…AC-11,
> incl. AC-08a/AC-08b). Component test plans map 1:1 to the three vnc-040 components:
> `slug_config_classification`, `resolve_slug_config`, `per_slug_loop`. Reused functions
> (`merge_configs`, `load_single_config`, `validate_config`, `build_project_server`) are exercised
> THROUGH these components, not given their own plans (per Component Map, IMPLEMENTATION-BRIEF.md).

## 1. Test Strategy

vnc-040 is a **Rust-internal, startup-time** feature: a per-key config overlay resolved at the
`build_project_server` call site, plus a declarative classification registry and a machine-checked
drift-guard. The behavior that matters — `Arc::ptr_eq` fallthrough, one-model-each at N≥2,
`merge_configs` overlay-vs-lock fidelity, post-merge cross-field re-validation, fail-loud-at-startup —
is almost entirely **below the MCP surface**. The test pyramid is therefore weighted to Rust unit +
construction-proof tests, with a deliberately thin integration layer (see §5).

| Layer | Vehicle | Weight | What it proves |
|-------|---------|--------|----------------|
| Unit (merge / classification) | `infra/config.rs` test module; `#[test]` | **Heavy** | `merge_configs` overlay-vs-lock per key (AC-03/05/11), post-merge validation (AC-08b), the registry drift-guard, exhaustiveness vs `validate_config` |
| Unit (resolution helper) | `resolve_slug_config` test module (call-site crate) | **Heavy** | no-file fallthrough, file-present order, error naming, DoS/perm hardening reuse (AC-02/08a) |
| Behavioral N=2 (model-free) | `#5172` model-free harness, in-crate Rust integration test | **Medium** | per-slug isolation (categories AC-01, instructions AC-10), one NLI + one embed handle at N≥2 (AC-04) |
| Construction proof / review | code-review checklist asserted in report | **Medium** | unconditional `Arc::clone` site (R-04), no-merge no-file arm (R-03), transport never read (R-09), restart-only (AC-09) |
| MCP integration (infra-001) | `pytest suites/` | **Thin / gate-only** | NO REGRESSION at the MCP surface; smoke gate. The per-slug overlay is not MCP-reachable in the current single-server harness (see §5) |

**Determinism:** all merge/classification/validation tests are pure (no I/O beyond a `tmp` config
file); N=2 model-free tests use the `#5172` harness (no real model loaded — handles are unloaded
sentinels whose `Arc` identity is asserted). No flaky surface.

## 2. Risk → Scenario → Component → AC Mapping

| Risk | Pri | Scenarios | Owning component test plan | AC |
|------|-----|-----------|-----------------------------|-----|
| R-01 | **Critical** | 4 | `resolve_slug_config` (post-merge `validate_config(&merged)`) | AC-08b |
| R-02 | High | 3 | `slug_config_classification` (inference-arm coverage) + `resolve_slug_config` | AC-03 |
| R-03 | High | 3 | `resolve_slug_config` (no-file arm) + `per_slug_loop` (`Arc::ptr_eq`) | AC-02 |
| R-04 | High | 3 | `per_slug_loop` (unconditional clone) + N=2 harness | AC-04 |
| R-05 | High | 2 | `slug_config_classification` (`*_sha256` global-wins + warn) | AC-05 |
| R-06 | Medium | 2 | `per_slug_loop` (forward guard on `VectorConfig::default()`) | AC-04 |
| R-07 | High | 2 | `slug_config_classification` (closed-checklist row-set) | AC-07 |
| R-08 | Low | 2 | `slug_config_classification` (nli_top_k/nli_enabled overlayable) | AC-03/07 |
| R-09 | Medium | 2 | `per_slug_loop` (transport never read at seam) | AC-06 |
| R-10 | Medium | 2 | `resolve_slug_config` (64 KiB cap + 0o022 perm) | AC-08a |
| R-11 | Low | 2 | `resolve_slug_config` (slug-named, startup-fatal) | AC-08a |
| R-12 | Medium | 2 | `per_slug_loop` + N=2 harness (instructions overlay/fallthrough) | AC-10 |
| R-13 | Low | 1 | `slug_config_classification` (single-owner / B-renders-from-A, doc-asserted) | — (residual) |
| R-14 | **High (proof obligation)** | 2 | `slug_config_classification` (drift-guard) | AC-11 |

All 14 risks and 32 scenarios are claimed by exactly one owning component plan; AC-09 (restart-only)
and FR-13 (`adapt` default) are construction/review-only with no behavioral test (verified C).

## 3. Cross-Component Test Dependencies

- **`resolve_slug_config` ↔ `per_slug_loop` boundary (the integration hazard, A3):** the helper
  returns `Cow<UnimatrixConfig>`; the loop clones fields 0–2 OUTSIDE/AHEAD of the call and derives
  fields 3–9 + instructions FROM the result. R-03 (no-merge no-file arm) and R-04 (unconditional
  clone) straddle both — the `Arc::ptr_eq` assertion lives in `per_slug_loop`, the `Cow::Borrowed`
  return lives in `resolve_slug_config`. Both plans reference the same assertion so neither side is
  tested in isolation only.
- **`slug_config_classification` ↔ `merge_configs` (AC-11):** the drift-guard test in
  `slug_config_classification` drives `merge_configs` directly — it is the binding that pins the
  registry to real merge behavior. `resolve_slug_config` consumes `merge_configs` too, so an arm flip
  fails BOTH the drift-guard (loudly, naming the key) and the helper's overlay tests (behaviorally).
- **The closed-checklist row-set (R-07 / AC-07):** must be machine-derivable from the LIVE
  `build_project_server` signature. `slug_config_classification` owns the row-set test; `per_slug_loop`
  owns the proof that each row's value is actually threaded (or not) at the call site. A new
  call-site argument must break the checklist test (`slug_config_classification`) loudly.

## 4. Cross-Field Invariant Enumeration (AC-08b prerequisite — MANDATORY for Stage 3c)

R-01/AC-08b requires AT LEAST ONE merged-only violation per cross-field invariant in
`validate_config`. Before writing R-01 tests, Stage 3c MUST enumerate every sum/cross-field
constraint from `validate_config` (`infra/config.rs:3413`). Known classes from ARCH §5:
1. Fusion-weight sum-of-six ≤ 1.0 (`[inference]` weights) — the canonical #3905 case.
2. PPR weight constraints.
3. Confidence weight constraints.
4. Custom-preset cross-level inheritance prohibition (#3923).
5. Any category / instruction size or well-formedness bound.

For EACH: construct a global+per-slug pair each individually valid whose MERGE violates it, prove
startup fails naming the slug, AND prove per-file-only validation does NOT catch it. The enumeration
itself is a recorded obligation in RISK-COVERAGE-REPORT.md (a gap if any class is unenumerated).

## 5. Integration Harness Plan (infra-001)

### 5a. Suite applicability

Per the suite-selection table, vnc-040 "touches schema/storage config" and "any server tool logic"
indirectly, so the relevant suites are:

| Suite | Run in 3c? | Rationale |
|-------|-----------|-----------|
| `smoke` (`-m smoke`) | **YES — mandatory gate** | Any change at all. Proves the binary still handshakes/serves after the call-site loop change. |
| `protocol` | YES | The call-site loop builds every served project; a regression could break startup/handshake. |
| `tools` | YES | crt-056 params (categories, confidence, inference) feed `ServiceLayer`; a wrong derivation would surface in tool responses. |
| `lifecycle` | YES | Schema/storage-adjacent (store→search, restart persistence); the per-slug seam runs at the restart that re-attaches routing. |
| `confidence` | YES | `confidence_params` is a per-slug-derived value; verify the global/no-file path still yields correct base scores. |
| `volume`, `security`, `contradiction`, `edge_cases` | OPTIONAL | Run if time permits; not implicated by the per-slug seam. `edge_cases` recommended (empty-DB / boundary regressions). |

### 5b. Coverage gap — the per-slug overlay is NOT MCP-reachable today

**Finding (load-bearing for Stage 3c triage):** the infra-001 harness drives a SINGLE STDIO server
with one `--project-dir` (`harness/client.py:105`); it has no multi-slug / `base_dir` /
HTTP-multi-project fixture and no per-slug `{base_dir}/{slug}/config.toml` placement helper. The
vnc-040 behaviors — `Arc::ptr_eq` fallthrough, one-model-each at N≥2, `merge_configs` drift-guard,
post-merge re-validation — are Rust-internal and **cannot be observed through the current single-server
MCP interface.** They are therefore covered by Rust unit + the in-crate `#5172` model-free N=2 harness,
NOT by infra-001. infra-001's role for vnc-040 is **regression-only**: prove the single-project /
no-file path (the silent majority, AC-02/NFR-02) still behaves byte-for-byte at the MCP surface.

### 5c. New integration tests to add (Stage 3c)

Per the "when to plan new integration tests" rule, vnc-040 adds **NO new tool, no new tool parameter,
and no new MCP-visible lifecycle flow** — the overlay is invisible at the single-server MCP surface.
Therefore:

- **No additions to `suites/test_tools.py`, `test_lifecycle.py`, `test_security.py`, or
  `test_confidence.py` are planned.** The per-slug behavior is not MCP-visible in the current harness;
  adding speculative single-server tests would be vacuous.
- **One regression assertion is in scope IF the smoke run reveals a single-project startup change:**
  the no-file global path must serve identically. If `smoke`/`tools`/`confidence` pass unchanged, AC-02
  is corroborated at the MCP surface (the Rust `Arc::ptr_eq` test is the authoritative proof).
- **Harness infrastructure for true multi-slug per-slug-config testing is a significant harness change
  → file a GH Issue (per agent-definition guidance), do NOT build it in this feature PR.** Recommended
  issue: "infra-001: add multi-slug HTTP fixture + per-slug config.toml placement for vnc-040/Feature-B
  integration coverage." This unblocks Feature B (seeding) and future per-slug MCP-level assertions.

### 5d. Failure triage posture (Stage 3c)

Any infra-001 failure is triaged per USAGE-PROTOCOL.md: caused-by-this-feature → fix + re-run;
pre-existing/unrelated → GH Issue + `@pytest.mark.xfail(reason="Pre-existing: GH#NNN — …")`, never
fixed in this PR; bad assertion → fix the test + document. Because vnc-040 changes only the per-slug
derivation (not the single-project path), a NEW failure in `tools`/`confidence`/`lifecycle` on the
single-server path is a strong signal the no-file fallthrough regressed (R-03) — treat as
caused-by-this-feature and investigate the `Cow::Borrowed`/`Arc::ptr_eq` arm first.

## 6. Anchored High-Value Obligations (explicit)

These obligations are called out so no Stage-3c agent treats them as optional. Each is owned by a
component plan and re-stated here:

1. **AC-11 drift-guard (R-14, High):** for EVERY `PER_SLUG_CONFIG_CLASSIFICATION` entry, drive
   `merge_configs` with a global+per-slug pair differing only on that key; assert
   `PerSlugOverlayable`⇒slug wins, `GlobalLocked`⇒global wins (incl. `*_sha256` carve-out). PLUS assert
   the registry key list is EXHAUSTIVE vs `validate_config`'s field set. → `slug_config_classification`.
2. **AC-08b post-merge re-validation (SR-01/R-01, Critical):** a global+per-slug pair each individually
   valid whose MERGE violates a cross-field invariant (sum-of-six > 1.0) fails loud at startup naming
   the slug; per-file-only validation provably does NOT catch it. → `resolve_slug_config`.
3. **AC-02 `Arc::ptr_eq` fallthrough:** assert `Arc::ptr_eq` on the 3 global handles on the no-file arm
   (not value-equality). → `per_slug_loop`.
4. **AC-04 model invariants:** N=2 model-free harness (#5172), exactly one NLI + one embedding handle at
   N≥2; construction-review the unconditional clone site. → `per_slug_loop` + N=2 harness.
5. **AC-10 instructions overlay + AC-01 categories overlay** behavioral N=2. → `per_slug_loop` + harness.
6. **R-06 forward guard:** standing test that fails if per-slug vector dims ever become config-driven
   (currently `VectorConfig::default()`, `http_provision.rs:182`). → `per_slug_loop`.
7. **R-10 DoS/permission hardening:** 64 KiB cap (#2395) + `#[cfg(unix)]` `mode() & 0o022` exercised on
   the per-slug path, not assumed. → `resolve_slug_config`.

## 7. Self-Check

- [x] OVERVIEW maps every risk (R-01…R-14) to scenario(s) and an owning component.
- [x] Integration harness section identifies applicable suites, the MCP-reachability gap, and the
      "no new MCP tests + file GH Issue for multi-slug fixture" decision.
- [x] Per-component plans map 1:1 to architecture components.
- [x] Every High/Critical risk has ≥1 concrete test expectation in its owning plan.
- [x] Integration/boundary tests defined (`resolve_slug_config`↔`per_slug_loop`,
      `classification`↔`merge_configs`).

## Knowledge Stewardship
- Queried: `context_briefing` (#5210/#5206/#5199/#5209 vnc-040 ADRs surfaced) + `context_search`
  (config-merge / N=2 isolation) — confirmed ADR-004 single-owner classification + ADR-003 post-merge
  re-validation + ADR-002 by-construction invariants; no test-pattern entry beyond #5172 (model-free
  N=2 harness) and #4070 (hidden merge literal) was missing.
- Stored: nothing novel at plan time — the test patterns (post-merge re-validate, `Arc::ptr_eq`
  fallthrough sentinel, registry-pins-merge drift-guard) are already captured by #3905, crt-056 AC-2,
  and crt-031. A reusable "single-server MCP harness cannot reach a multi-slug startup seam → unit +
  N=2-harness, file GH Issue for harness uplift" lesson is a candidate for Stage 3c if it recurs.
