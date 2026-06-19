# Risk-Based Test Strategy: vnc-040

> Per-Slug Configuration Overlay Resolution (C6 / Feature A of #785). Risks are SPECIFIC to
> the approved design: the new `resolve_slug_config` helper, the FULL `build_project_server`
> call-site surface (~12 inputs: embed_handle, permissive, instructions + 9 crt-056 params),
> the unconditional `Arc::clone` of the 3 global handles, the per-slug-OVERLAYABLE `instructions`,
> the GLOBAL-LOCKED `permissive` daemon flag, the `Cow` fallthrough, the post-merge
> `validate_config`, and the `merge_configs` reuse. Historical evidence cited as Unimatrix entry IDs.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Post-merge `validate_config(&merged)` omitted or placed wrong; cross-level invariant (fusion-weight sum-of-six > 1.0) passes both per-file checks yet violates the merged struct (#3905) | High | High | **Critical** |
| R-02 | `merge_configs` inline `InferenceConfig {…}` literal (the hidden 5th site, #4070) drops/mishandles a field for the global→per-slug call shape — silent compile, wrong merge | High | Med | **High** |
| R-03 | Fallthrough is not byte-for-byte: `Cow::Borrowed` no-file arm re-derives Arcs via a merge instead of reusing the daemon's parity Arcs, silently changing behavior for the single-project majority (#4583) | High | Med | **High** |
| R-04 | A field 0–2 handle (`nli_handle`/`embed_handle`/`rayon_pool`) sourced from `resolved` inside a branch instead of `Arc::clone`d unconditionally — loads/selects a 2nd model, breaks crt-056 AC-2 | High | Med | **High** |
| R-05 | Hash-pin global-wins (`*_sha256`) regresses under the global→per-slug pairing; per-slug pin smuggles a different model descriptor; warn not emitted (#4655/#4649/#4648) | High | Low | **High** |
| R-06 | Forward-guard breach: a later change makes the per-slug vector index config-driven (not `VectorConfig::default()`), re-opening the `[embedding]` config-vs-handle divergence (SR-03, A2, #5196) | High | Low | **Medium** |
| R-07 | Verdict checklist drops/mis-classifies a `build_project_server` call-site input — this risk ALREADY MATERIALIZED TWICE in design-gate (first `embed_handle`, then `instructions`/`permissive` omitted); the full surface is ~12 (9 crt-056 params + embed_handle + permissive + instructions), not "10" — mirror of crt-056 AC-1 gap | Med | High | **High** |
| R-08 | `nli_top_k`/`nli_enabled` (fields 3–4) treated as model-coupled, or an out-of-range per-slug override misbehaves against the shared handle | Med | Low | **Low** |
| R-09 | Transport key in a per-slug file silently read/threaded at the seam (TLS/auth/host/`http.enabled`) — privilege/exposure change per slug | Med | Low | **Medium** |
| R-10 | Per-slug file load skips the existing 64 KiB cap (#2395) or `#[cfg(unix)]` 0o022 permission check — DoS / world-writable config trust-boundary bypass (NFR-03/04) | Med | Low | **Medium** |
| R-11 | Error from `resolve_slug_config` does not name the offending slug, or fails at request time instead of startup — silent degradation (NFR-05, #4583) | Med | Low | **Low** |
| R-12 | Per-slug `instructions` overlay regresses: slug's served instructions do not reflect its file, or absent-file slug stops falling through to the global `resolved.server.instructions` (#785's explicit ask; was global at `main.rs:687`/`1095`) | Med | Med | **Medium** |
| R-13 | A per-slug file setting a GLOBAL-only section (e.g. `[server.tls]`, `permissive`) is silently ignored at the seam — the silent-ignore (no runtime warn) stays the accepted 🟠 residual; but the per-slug-vs-global split is now OWNED by A's canonical classification (source of truth), not hand-duplicated — B renders from it | Low | Med | **Low (accepted residual; split now owned in A)** |
| R-14 | The canonical classification (Feature A, source of truth) silently disagrees with `merge_configs`' actual overlay-vs-lock behavior, or with Feature B's seed rendering — the crt-031 multi-copy-divergence pattern: a hand-maintained doc/table drifts from the code it describes | High | Med | **High (proof obligation)** |

## Risk-to-Scenario Mapping

### R-01: Post-merge cross-field invariant gap (Critical)
**Severity**: High · **Likelihood**: High · **Impact**: A slug runs with an arithmetically-invalid
fusion/PPR config (sum > 1.0); scoring is silently corrupted for that tenant. The exact crt-024
recurrence (#3905); ADR-003/#5199 prescribes the fix.
**Test Scenarios**:
1. Global resolved config sets some `[inference]` weights non-default; per-slug file sets *other*
   weights non-default — each VALID alone, merge sum > 1.0 → startup error naming the slug (AC-08b/FR-07).
2. Same shape against PPR / confidence weight constraints and the custom-preset cross-level prohibition (#3923).
3. Verify `validate_config(&merged, &path)` runs INSIDE `resolve_slug_config`, AFTER `merge_configs`,
   BEFORE return — not only the per-file call (construction proof, AC-08b).
4. Negative: a merged config whose sums are valid passes (no false-positive startup failure).
**Coverage Requirement**: At least one merged-only sum-violation per cross-field invariant in
`validate_config`; the spec/tester must enumerate them exhaustively (sum-of-six, PPR, confidence,
preset, size bounds). Per-file-only validation must demonstrably FAIL to catch the merged violation.

### R-02: Hidden `merge_configs` literal drift (High)
**Severity**: High · **Likelihood**: Med · **Impact**: A field silently falls to a wrong value
in the merged struct for the new caller; invisible at compile time (#4070).
**Test Scenarios**:
1. For EACH overlayable `[inference]` field, a per-slug override is reflected in the merged config
   and a non-overridden sibling falls through to global (per-key, AC-03).
2. Re-audit gate: confirm the inline `InferenceConfig {…}` literal lists every field explicitly or
   ends `..InferenceConfig::default()` (delivery runs the #4070 grep; risk requires it be asserted).
3. Confirm the global→per-slug call shape exercises the SAME arm as global→project (no project-only assumption).
**Coverage Requirement**: Every `InferenceConfig` field exercised through the merge under the C6
call shape; the inline-literal audit recorded as a checked obligation.

### R-03: Fallthrough not byte-for-byte (High)
**Severity**: High · **Likelihood**: Med · **Impact**: Behavior change for every local UDS /
single-project user (the silent majority) when no per-slug file exists — the highest blast-radius
regression (#4583).
**Test Scenarios**:
1. No per-slug file: per-slug-resolved config and all ~12 threaded inputs == the global-only crt-056
   path, byte-for-byte (AC-02/R).
2. Construction proof: the no-file arm returns `Cow::Borrowed(&global)` / passes global Arcs
   unchanged — NO `merge_configs`, NO re-derivation runs (AC-02/C).
3. Machine-checked pointer identity: `Arc::ptr_eq` on the no-file arm for the 3 global handles
   (embed/nli/pool) — they ARE the daemon's already-built parity Arcs, not freshly constructed.
   Matches crt-056 AC-2's `Arc::ptr_eq`; converts the no-re-derivation guarantee from review-only to
   machine-checked (per ARCHITECTURE §3 optimization).
**Coverage Requirement**: Equality assertion across all ~12 inputs in the no-file path PLUS an
`Arc::ptr_eq` assertion on the 3 global handles, NOT a review note — the no-merge guarantee is now
machine-checked.

### R-04: Model handle sourced from merged config (High)
**Severity**: High · **Likelihood**: Med · **Impact**: A 2nd NLI or embedding model loads at N≥2
slugs; breaks crt-056 AC-2 / NFR-01; memory blowup, model divergence.
**Test Scenarios**:
1. N=2 model-free harness (#5172): exactly one NLI handle and one embedding handle resident at N≥2,
   distinct slug configs (AC-04/B).
2. Construction proof: fields 0–2 `Arc::clone`d UNCONDITIONALLY, outside/ahead of
   `resolve_slug_config`, never read from `resolved` on any branch (AC-04/C).
3. A per-slug file attempting `[embedding].model` / model-identity keys leaves the served handle and
   merged descriptor as the global model (no load, no describe).
**Coverage Requirement**: One-handle-each assertion at N≥2 AND construction review of the loop's
field-0–2 clone site.

### R-05: Hash-pin global-wins regression (High)
**Severity**: High · **Likelihood**: Low · **Impact**: A per-slug file overrides an operator hash
pin → supply-chain model-substitution bypass (#4655/#4649). Extending the control to a parallel
path can expose a latent flaw (#4648).
**Test Scenarios**:
1. Global pin set + differing per-slug `embedding_model_sha256`/`nli_model_sha256` → merged == global
   pin; `tracing::warn` naming the divergence (AC-05/U).
2. No global pin + per-slug pin → per-slug pin does NOT silently become authoritative for a model
   the handle is not (descriptor lock, AC-04).
**Coverage Requirement**: Global-wins proven for both pin fields under the global→per-slug pairing;
warn asserted.

### R-06: Forward-guard breach on `VectorConfig::default()` (Medium)
**Severity**: High · **Likelihood**: Low · **Impact**: If per-slug vector dims become config-driven
later, a per-slug `[embedding].dimensions` could describe a model the handle is not — the divergence
SR-03/A2 is only defused today by the index using `VectorConfig::default()` (#5196).
**Test Scenarios**:
1. Assert the per-slug vector index is constructed from `VectorConfig::default()` (`http_provision.rs:182`),
   NOT from merged-config dims — a guard test that fails if a future change wires dims through.
2. Merged `[embedding]` section (today: `embedding_model_sha256`) == global section for any per-slug input (AC-04).
**Coverage Requirement**: A standing guard test pinning the `VectorConfig::default()` dependency, so
the A2 assumption breaks loudly if violated.

### R-07: Verdict checklist drops a call-site input (High)
**Severity**: Med · **Likelihood**: High · **Impact**: A silently-mis-classified input ships
un-noticed (the crt-056 AC-1 failure mode this checklist exists to prevent). This risk MATERIALIZED
TWICE during design-gate — first `embed_handle` was omitted from the verdict, then `instructions`
and `permissive` — so the proof obligation now covers the ENTIRE `build_project_server` call-site
surface, not a count.
**Test Scenarios**:
1. One verdict ROW per call-site input, derived by enumerating EVERY argument actually passed at the
   `build_project_server` call site (`main.rs:687`/`1095`): the 9 crt-056 params, `embed_handle`,
   `permissive` (GLOBAL-LOCKED), and `instructions` (per-slug OVERLAYABLE). Global-locked inputs
   proven not overlayable; overlayable inputs proven overlayable (AC-07/U+B).
2. Closed-checklist guard: the test asserts the verdict row-set is EXACTLY the call-site argument
   set — NONE absent — and fails if an argument exists with no row. No "10 fields" / "all 9 params"
   shorthand; the count is whatever the live call site is (~12).
**Coverage Requirement**: One verdict assertion per call-site input with zero arguments unclassified;
the row-set must be machine-derivable from / cross-checked against the live call site, so a future
added argument breaks the test loudly rather than silently shipping un-classified.

### R-08: nli_top_k / nli_enabled mis-coupling (Low)
**Severity**: Med · **Likelihood**: Low · **Impact**: Runtime-param overlay misbehaves against the
shared handle, or is wrongly locked global.
**Test Scenarios**:
1. Per-slug override of `nli_top_k`/`nli_enabled` reflected in the slug's `ServiceLayer` query
   behavior; the shared `nli_handle` is unchanged (AC-03/AC-07).
2. Confirm neither field selects/reloads a model (6c construction note).
**Coverage Requirement**: Both fields proven overlayable-as-runtime-param, model-handle untouched.

### R-09: Transport read at the seam (Medium)
**Severity**: Med · **Likelihood**: Low · **Impact**: A per-slug file changing TLS/auth/host/
`http.enabled` would alter the security posture per slug — out of the C6 `done_when`.
**Test Scenarios**:
1. Per-slug file setting transport keys → served transport == global; the slug loop never reads a
   transport field from `resolved` (AC-06/U+C).
2. Review: HTTP listener built from global config before the per-slug loop runs.
**Coverage Requirement**: Transport-unaffected assertion + seam review.

### R-10: Per-slug load bypasses DoS / permission hardening (Medium)
**Severity**: Med · **Likelihood**: Low · **Impact**: A world/group-writable or oversized per-slug
file is accepted — trust-boundary bypass at the new untrusted-input surface (NFR-03/04).
**Test Scenarios**:
1. Oversized (>64 KiB) per-slug file rejected before `toml::from_str` via `load_single_config` (#2395).
2. `#[cfg(unix)]` world/group-writable per-slug file (`mode() & 0o022 != 0`) rejected at startup.
**Coverage Requirement**: The reuse of `load_single_config`'s cap + permission check is exercised on
the per-slug path, not assumed.

### R-11: Error not slug-named / not fail-fast (Low)
**Severity**: Med · **Likelihood**: Low · **Impact**: Operator cannot identify which slug file is
bad; or failure surfaces at request time as silent degradation (#4583).
**Test Scenarios**:
1. Each invalid input class (unknown category, oversized instructions, malformed TOML) →
   `ServerError::Config` at STARTUP naming the offending slug file (AC-08a).
2. No `.unwrap()` on the per-slug path; failure is loud, not a request-time fallback.
**Coverage Requirement**: Every error path names the slug and fires at startup.

### R-12: `instructions` overlay regression (Medium)
**Severity**: Med · **Likelihood**: Med · **Impact**: #785 explicitly asked for per-slug
`instructions`; the design now sources it from the merged `resolved.server.instructions` (was global
at `main.rs:687`/`1095`). A regression either fails to apply a slug's file or breaks the absent-file
global fallthrough — the served MCP `instructions` is wrong per tenant.
**Test Scenarios**:
1. Behavioral overlay (N=2, model-free): slug A's served `instructions` reflect A's file; slug B's
   file differs → B's served `instructions` differ from A's. The two are independent, neither leaks
   into the other (AC-03/AC-07).
2. Absent-file fallthrough: a slug with no per-slug `instructions` key falls through to the global
   `resolved.server.instructions` (the `main.rs:687`/`1095` value), not empty/default.
**Coverage Requirement**: Per-slug `instructions` proven overlayable AND proven to fall through to
global when absent — both arms, no model load involved.

### R-13: Per-slug GLOBAL-only section silently ignored — ACCEPTED RESIDUAL, split now owned in A (Low)
**Severity**: Low · **Likelihood**: Med · **Impact**: A per-slug file setting a section that is
GLOBAL-only (e.g. `[server.tls]`, `permissive`) is read into the merged struct but never threaded at
the seam — silently ignored. ONLY the `*_sha256` pin path emits a warn (R-05); every other
global-only key is dropped with no diagnostic. An operator may believe a per-slug TLS/permissive
setting took effect when it did not.
**Resolution**: ACCEPTED RESIDUAL for the *runtime warn* — the silent-ignore behavior is NOT warned
at runtime; that part stays deferred per the earlier 🟠 call. No seam warn / field-comparison
machinery is built now.
**What changed (Option 2):** the per-slug-vs-global split is no longer an unowned, hand-duplicated
residual. Feature A now OWNS a SINGLE CANONICAL classification (source of truth) of which sections
are per-slug-overlayable vs global-locked. The verdict table (R-07) and Feature B's future seed
annotations both RENDER from that classification rather than re-stating it. The DOCUMENTATION OWNER
is now A's classification; Feature B is a CONSUMER/renderer, not a parallel author.
**Mitigation**: Feature B's annotated seed `config.toml` renders section ownership FROM A's canonical
classification, so operators do not place global-only keys per slug — and the rendering cannot drift
from A's source of truth by hand. The drift between the classification and real `merge_configs`
behavior is closed by R-14's drift-guard test. A runtime seam warn remains an OPTIONAL future
enhancement, not in scope for Feature A.
**Test Scenarios**:
1. Confirm A's canonical classification is the single source of truth in ARCH/Spec, the verdict table
   (R-07) is rendered/derived from it, and Feature B's seed annotation is named as a CONSUMER of it
   (not a hand-duplicated second copy). The behavioral guard against drift lives in R-14, not here.
**Coverage Requirement**: The split's correctness-against-code is now machine-checked by R-14. R-13's
own residual (no runtime warn for an ignored global-only key) remains documented, not test-gated.

### R-14: Classification ↔ `merge_configs` ↔ seed-render drift (High — proof obligation)
**Severity**: High · **Likelihood**: Med · **Impact**: A's canonical per-slug-vs-global
classification silently disagrees with what `merge_configs` actually does at the seam (a section
classified "per-slug-overlayable" that the merge in fact locks, or vice-versa), or Feature B's seed
rendering diverges from the classification. This is the crt-031 multi-copy-divergence pattern: a
hand-maintained source-of-truth doc rots away from the code it claims to describe, and operators
trust a verdict/annotation that no longer matches runtime behavior. Making the classification
canonical only HELPS if a machine pins it to reality — otherwise it is a more authoritative lie.
**Test Scenarios**:
1. **Machine-checked drift guard (the proof obligation):** for EVERY `build_project_server`
   call-site input, assert `merge_configs`' actual overlay-vs-lock behavior matches A's canonical
   classification — an overlayable-classified field is observably overlaid by a per-slug override; a
   global-locked-classified field is observably NOT overlaid (stays global) under the same override.
   The test enumerates from the live call site (same closed-set discipline as R-07), so a future
   classification entry with no matching merge behavior — or a merge change with no classification
   update — fails loudly.
2. Render parity: assert Feature B's seed annotation (when it lands) is generated from / cross-checked
   against A's canonical classification, so the seed cannot state an ownership that the classification
   does not — no third hand-kept copy.
**Coverage Requirement**: One drift-guard assertion per call-site input tying the classification to
`merge_configs`' real behavior; the classification is machine-pinned to code, not review-asserted.
This is the proof obligation that prevents the multi-copy divergence A's source-of-truth design
otherwise risks.

## Integration Risks

- **Seam stability (A3):** the whole strategy is derived from the LIVE `build_project_server`
  call-site surface (~12 args: 9 crt-056 params + embed_handle + permissive + instructions), not the
  issue's "8 fields" nor an interim "10". R-07 just materialized TWICE because the checklist was
  derived from a count instead of the live call site — so the verdict row-set must be enumerated FROM
  the call site and re-derived if the signature shifts. Test against the actual call-site arguments.
- **`resolve_slug_config` ↔ loop boundary:** the helper returns a `Cow`/`UnimatrixConfig`; the loop
  derives 6 Arcs from it and clones 3 outside it. The split is the integration hazard — fields 0–2
  must be cloned in the loop OUTSIDE/AHEAD of the helper call (R-04), and the no-file `Cow::Borrowed`
  arm must not trigger Arc re-derivation (R-03). Both are construction-proof obligations, not just behavioral.
- **`merge_configs` reuse asymmetry (A1):** the function is used for a third layer it was not written
  for; the `inference` arm's inline literal is the drift site (R-02). Re-audit before trusting reuse.
- **Canonical classification ↔ code ↔ seed-render (Option 2, crt-031):** A now owns a single
  source-of-truth per-slug-vs-global classification; the verdict table (R-07) and Feature B's seed
  annotations RENDER from it. The integration hazard is divergence between that classification and
  `merge_configs`' real overlay-vs-lock behavior (or B's render) — closed by the R-14 drift-guard,
  which machine-pins the classification to per-call-site merge behavior rather than trusting a
  hand-kept doc.

## Edge Cases

- Per-slug file present but EMPTY / all-default → merged == global (degenerate fallthrough; must not differ from no-file path semantics).
- Per-slug file overrides a list field (`categories`/`boosted_categories`) → REPLACE not append (#2286 replace semantics, AC-03).
- `Option` field set in global, unset in per-slug → global value retained via `.or()` (R-02 sibling).
- N=0 / N=1 slugs (no multi-tenant) → no behavior change, fallthrough path only (R-03).
- Per-slug file at the path but unreadable (permissions) → fail-loud naming slug (R-10/R-11).
- A per-slug file setting `[adapt]` (FR-13: left default) → no effect; not threaded.
- Per-slug `instructions` set in global, unset per-slug → global `resolved.server.instructions` retained (R-12 fallthrough).
- Per-slug file setting a GLOBAL-only section (`[server.tls]`, `permissive`) → silently ignored except `*_sha256` warn (R-13 accepted residual).

## Security Risks

- **Untrusted input:** `{base_dir}/{slug}/config.toml` is operator-hand-placed (Feature A) but is a
  NEW file-based input surface read at startup. Threats: (a) oversized file DoS — mitigated by the
  64 KiB cap (R-10); (b) world/group-writable file → tampering — mitigated by the `#[cfg(unix)]`
  0o022 check (R-10); (c) malformed TOML → parse panic — must fail-loud, no `.unwrap()` (R-11);
  (d) **model-substitution via hash-pin override** — the highest-value attack; defused by global-wins
  pins (R-05) AND the embedding-descriptor lock (R-04/R-06).
- **Blast radius:** confined to ONE slug's `ServiceLayer` at startup; a bad file fails the daemon
  loud (no partial-serve). It cannot reach transport (R-09), cannot load a 2nd model (R-04), and
  cannot alter another slug or the global config. The fail-fast-at-startup design keeps the blast
  radius at "daemon refuses to start," never "silently degraded at request time."
- **No new attack surface beyond the file:** no new endpoint, no reload watcher (AC-09), no network input.

## Failure Modes

| Condition | Required behavior |
|-----------|-------------------|
| Malformed TOML / unknown category / oversized instructions in per-slug file | Loud `ServerError::Config` at STARTUP naming the slug file; daemon refuses to start (R-11/AC-08a) |
| Per-file-valid but merge violates cross-field invariant | Post-merge `validate_config(&merged)` fails loud at startup naming the slug (R-01/AC-08b) |
| Per-slug hash pin differs from global pin | Global pin wins; `tracing::warn` logs the divergence; no failure (R-05/AC-05) |
| World/group-writable or >64 KiB per-slug file | Rejected at load by reused hardening; startup fails (R-10) |
| No per-slug file | Silent, correct fallthrough to global — byte-for-byte, no merge (R-03/AC-02) |
| Transport key present in per-slug file | Ignored at seam; global transport unchanged (R-09/AC-06) |
| Global-only section (`[server.tls]`, `permissive`) present in per-slug file | Silently ignored at seam, no runtime warn (accepted residual R-13); only `*_sha256` pin warns. The per-slug-vs-global split is owned by A's canonical classification; Feature B seed renders ownership FROM it (R-13). Drift between classification and real merge behavior is guarded by R-14 |
| Per-slug `instructions` set / absent | Overlaid when set; falls through to global `resolved.server.instructions` when absent (R-12/AC-03) |

No degraded/partial-serve mode exists: per-slug config errors are startup-fatal, never request-time.

## Scope Risk Traceability

| Scope Risk | Architecture / Spec element that addresses it | Architecture Risk | Test scenario that proves it |
|-----------|-----------------------------------------------|-------------------|------------------------------|
| SR-01 (post-merge cross-level gap, #3905) | ARCH §5 + ADR-003/#5199: `validate_config(&merged)` inside `resolve_slug_config`; FR-07/AC-08b | R-01 | R-01 #1–4 (merged-only sum violation; in-helper placement proof) |
| SR-02 (hidden `merge_configs` literal drift, #4070) | ARCH §10 A1 re-audit obligation; Spec A1; reuse `merge_configs` unchanged | R-02 | R-02 #2–3 (inline-literal audit; same-arm proof) |
| SR-03 (`[embedding]` carve-out necessary-not-sufficient, #4655/#5196) | ARCH §6b: only descriptor is `embedding_model_sha256` (already global-wins); no new descriptor field; FR-05/AC-04 | R-04, R-06 | R-04 #3 + R-06 #2 (merged `[embedding]` == global; no 2nd model described) |
| SR-04 (fallthrough regression, #4583) | ARCH §3 + §4 no-file arm: `Cow::Borrowed`, no merge/re-derive; FR-08/AC-02; NFR-02 | R-03 | R-03 #1–3 (byte-for-byte across 10 inputs; no-merge construction proof) |
| SR-05 (verdict checklist drops a call-site input) | ARCH §9 closed verdict table now RENDERED from A's single canonical per-slug-vs-global classification (source of truth), enumerated from the FULL `build_project_server` call site (~12 args incl. permissive GLOBAL-LOCKED + instructions OVERLAYABLE), not a count; FR-11/AC-07 | R-07, R-12, R-14 | R-07 #1–2 (one verdict row per call-site arg, none absent) + R-12 #1–2 (instructions overlay + fallthrough) + R-14 #1 (classification matches real `merge_configs` behavior per call-site input) |
| SR-06 (A/B split incomplete first pass, #2397) | ARCH §4 records shared `{base_dir}/{slug}/config.toml` path + FR-13 leave-`adapt`-default decision | — | Design-recorded contract; no test (decision artifact, not behavior) |
| SR-07 (crt-056 seam override-vs-handle coupling) | ARCH §6a: fields 0–2 `Arc::clone`d UNCONDITIONALLY outside merge; FR-04/AC-04 | R-04 | R-04 #1–2 (one-handle-each at N≥2; unconditional-clone construction proof) |
| SR-08 (nli_top_k/nli_enabled field- vs model-coupling) | ARCH §6c: runtime params, not model identity; FR-03/verdict rows 3–4 | R-08 | R-08 #1–2 (overlay reflected; handle untouched; no reload) |

All eight scope risks (SR-01…SR-08) trace to an architecture/spec element AND a proving scenario.
SR-06 is a design-recorded decision (no behavioral test); all others have at least one test scenario.

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|--------------------|
| Critical | 1 (R-01) | 4 |
| High | 6 (R-02, R-03, R-04, R-05, R-07, R-14) | 15 |
| Medium | 4 (R-06, R-09, R-10, R-12) | 8 |
| Low | 3 (R-08, R-11, R-13) | 5 |
| **Total** | **14** | **32** |

R-13's silent-ignore-without-runtime-warn remains an ACCEPTED RESIDUAL; its single scenario now
confirms A owns the canonical classification and B renders from it (source-of-truth ownership), not a
behavioral guard. The behavioral guard against classification↔code↔seed drift is R-14 — the machine-
checked proof obligation (the crt-031 pattern) that pins A's classification to `merge_configs`' real
overlay-vs-lock behavior per call-site input.

## Knowledge Stewardship
- Queried: `context_search` for cross-field-validation lessons and config-merge risk patterns —
  surfaced #3905 (post-merge re-validation, the SR-01 root), #4070 (hidden `merge_configs` literal,
  SR-02), #4655/#4649/#4648 (hash-pin global-wins + parallel-path flaw exposure, R-05), #5196
  (lock whole section describing a global handle, R-06), #4583 (silent-fallback regression, R-03/R-11),
  #2395/#2396 (64 KiB cap + Option absence-vs-zero, R-10/edge), #5172 (model-free N=2 harness, R-04).
  Confirmed ADR-003/#5199 already encodes the SR-01 post-merge-revalidation fix.
- Stored: nothing novel to store — the recurring risk pattern (third config layer must re-validate
  the merged result and re-audit the hidden `merge_configs` literal) is already captured by #3905
  and #4070, and the per-slug-specific decision is captured in ADR-003/#5199. No cross-2+-feature
  pattern emerged that those entries don't already cover.
