# Scope Risk Assessment: vnc-041

Config seeding (global + per-slug) + seam-level WARN for global-locked keys (C17 / vnc-040 Feature B).
Evidence drawn from Unimatrix #5206 (vnc-040 ADR-002), #5211 (drift-guard split), #4567 (config write path), #665 (file-write TOCTOU).

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Skip-if-exists not atomic — a check-then-write (TOCTOU) or `File::create` truncates an operator file before the guard fires (#665). | High | Med | Reuse `write_default_config_if_absent`'s `OpenOptions::create_new` no-clobber path (#4567); never `fs::write`/`File::create` on a seed write. One ADR for the seed-write primitive shared by global + per-slug. |
| SR-02 | "Locked surface" for the WARN is enumerated by hand and drifts from A's classification — `permissive` has NO `UnimatrixConfig` field, transport/embedding are construction-locked, hash pins are merge-locked (#5211). A hardcoded list will mis-warn or miss keys. | High | High | WARN surface MUST derive from `is_per_slug_overlayable` returning false — the same registry A owns. Do not restate the locked set in B. |
| SR-03 | Annotation renderer must reproduce A's classification field-for-field (AC-03) but `permissive` and the embedding descriptor have no clean field to render from (#5211). | Med | Med | Render annotations by iterating the classification registry, not by hand-listing keys; treat field-less locks (permissive) explicitly. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Global seed regresses the single-project/local majority. #5206 records #4583 shipped a silent-fallback config bug here; OQ-4 confines the seed to container `serve`, but the gate is the only thing holding it. **Note (ADR-004, #5238): the safeguard gate is `config.http.enabled`, not the `base_dir` argument (live `serve` always passes `base_dir = None`).** | High | Med | Make the container-only branch structural (decided by `ensure_data_directory`'s `base_dir = Some(/data)` arg), not a runtime flag; AC-06 sentinel must assert local `serve` writes zero files. |
| SR-05 | The shared (a)≡(c) path-hash `config.toml` is now written by TWO paths in the same `register` flow — `ensure_project_stanza` (c, vnc-038) and any global-seed touch — risking clobber of `[[projects]]` or global knobs. | High | Med | B's per-slug seed targets the DISTINCT file (b) `{base_dir}/{slug}/config.toml` only; B must NOT write (a)/(c). Architect: confirm the global seed (Goal 1) is `serve`-time, never inside `register`, so the two writers never overlap. |
| SR-06 | WARN scope creeps toward rejection or behavior change. R-13 is WARN-only (AC-04); a "helpful" validation could alter resolution. | Med | Low | Constrain to one `tracing::warn` per ignored key per boot; value stays ignored; no new error path. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | A→B classification drift: A changes `PER_SLUG_CONFIG_CLASSIFICATION`, B's seed annotations + WARN surface silently diverge. | High | Med | Single consumption point — both annotation render and WARN derive from `is_per_slug_overlayable` at runtime; add a test that a classification flip changes the seed annotation (AC-03 "proven, not restated"). |
| SR-08 | Variant/signature ripple: adding a seed path may touch `Command` variants or `handle_version`/`register` signatures, breaking `matches!`/call sites (#4567). | Med | Med | Audit `main_tests.rs` and all `Command` match arms before compile; prefer additive call sites over variant shape changes. |
| SR-09 | Per-slug seed dir is a SIBLING of the path-hash dir (`base_dir = paths.data_dir.parent()`); a wrong base computation writes (b) where the resolver can't read it. | Med | Low | Seed write MUST reuse the exact `resolve_slug_config` path construction (`base_dir.join(slug).join(PROJECT_CONFIG_NAME)`), not recompute. |

## Assumptions

- **A→B contract is stable and queryable at runtime** (SCOPE Goal 3, Constraints "A→B one-way"). If `is_per_slug_overlayable` is not callable from the seed/WARN sites, B is forced to restate the split — invalidating SR-02/SR-03/SR-07. Confirm the registry is accessible from both `register` and `resolve_slug_config`.
- **`register <slug>` is the sole per-slug provisioning point** (SCOPE OQ-2, Goal 2). If slugs can be created by any path other than `register`, the eager seed misses them.
- **Container vs local is decided structurally by `base_dir`** (SCOPE AC-01, Background (a)). If the container path is detected by a runtime heuristic instead, SR-04's regression guard is weaker.
- **No new config knobs** (SCOPE Non-Goals). If the seed introduces a knob, AC-06 byte-for-byte and the A-owned classification both break.

## Design Recommendations

1. **One shared no-clobber seed primitive** (`create_new`-based, #4567) for both global and per-slug writes — addresses SR-01, SR-05. (architect)
2. **Derive everything locked from `is_per_slug_overlayable` at runtime** — annotation render AND WARN surface — never a hand-list; add the classification-flip test (AC-03) — addresses SR-02, SR-03, SR-07. (spec writer: state as a constraint)
3. **Make container-only structural and assert local writes zero files** (AC-06 sentinel) — addresses SR-04. (architect + spec)
4. **Keep B off the shared (a)≡(c) file entirely**; B writes only (b) — addresses SR-05, SR-09. (architect)
5. **Audit `Command`/`handle_version`/`register` call sites before any signature change** (#4567) — addresses SR-08. (architect)
