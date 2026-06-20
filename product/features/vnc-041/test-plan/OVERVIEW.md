# vnc-041 Test Strategy — Config Seeding + Seam-Level WARN (C17 / vnc-040 Feature B)

> Sources: `RISK-TEST-STRATEGY.md` (R-01..R-14), `ACCEPTANCE-MAP.md` (AC-01..AC-06),
> `architecture/ARCHITECTURE.md`, ADR-001..005, `IMPLEMENTATION-BRIEF.md`.
> Crate under test: **`unimatrix-server`** (the binary crate). `unimatrix-engine` is a read-only dep.
> Verification command (per AC map): `cargo test -p unimatrix-server`.

## 1. Test Layers

| Layer | Where | What it proves |
|-------|-------|----------------|
| **Unit** (in-crate `#[test]`) | `infra/config.rs` mod-tests + new test modules per component | C1 no-clobber primitive, C2 render-from-registry, C5 WARN derivation. Deterministic, no daemon. |
| **Integration (in-crate)** | new test modules driving `ProjectRegistry::register` + `resolve_slug_config` round-trip, and the C4 seed function on a temp data dir | (b) lands at the resolver's exact path; register→resolve picks it up; (a)/(c) byte-unchanged. AC-02/AC-05/AC-03/AC-04. |
| **Feature-level (serve seam)** | C4 seed call exercised through the smallest reachable seam (the `write_default_config_if_absent` call + an `http.enabled` branch harness) | AC-01 fires with `base_dir = None`; AC-06 empirical zero-files sentinel + negative control. |
| **MCP integration harness (infra-001)** | `product/test/infra-001/suites/` | Regression gate only — see §5. vnc-041 is filesystem-provisioning + log behavior, NOT new tool surface; existing suites are the no-regression net, no new infra-001 tests required. |

This feature is almost entirely **Rust-side** (file writes, TOML render, `tracing` WARN). The
load-bearing tests are in-crate unit + integration tests. infra-001 is a regression gate, not the
primary proof surface (see §5 justification).

## 2. Test Conventions (reuse existing — cumulative, do not re-scaffold)

- **Temp dirs**: reuse `tempfile::TempDir` (the `projects/tests.rs` `Fixture` pattern) OR the
  zero-dep `TempBase` counter pattern in `http_provision/slug_config_tests.rs`. Prefer extending the
  existing fixtures — `Fixture` already exposes `base_dir`, `config_data_dir`, `slug_dir`, `set_routing`,
  and `ProjectRegistry::with_dirs`. The per-slug seed tests (C3) extend `Fixture`; the resolver-WARN
  tests (C5) extend `TempBase`.
- **`tracing` capture**: reuse `#[tracing_test::traced_test]` + `logs_contain(...)` exactly as
  `slug_config_classification_tests.rs::test_sha256_pins_global_wins_under_per_slug_pairing` does. This
  is the established pattern for asserting a WARN fired. For "exactly one / count" assertions use
  `traced_test`'s captured-log inspection (count occurrences of the message substring).
- **Naming**: `test_{component_or_concept}_{scenario}_{expected}` (workspace convention). The Feature A
  resolver tests use a `__<invariant>` suffix under `#![allow(non_snake_case)]`; C5 tests MAY follow that
  to keep the AC mapping legible, but standard snake_case is acceptable.
- **No daemon spin-up for C4** where avoidable: the C4 seed is `write_default_config_if_absent(path, false)`
  inside the `if config.http.enabled` block. Test the *gate placement* structurally (the seed is reachable
  only on the http.enabled branch) + the *empirical file-count* by invoking the seed function against a
  temp data dir under each branch's conditions. The full `tokio_main_daemon` boot is heavy; prefer the
  smallest reachable seam that still exercises the real call site (extend `per_slug_loop_tests.rs` /
  `main_tests.rs` patterns). If the http.enabled branch cannot be reached without a daemon boot, the
  empirical sentinel falls to an integration test that boots `serve` with a minimal config under each
  flag — but the file-count delta assertion (§AC-06) is MANDATORY regardless of harness depth.

## 3. Risk → Test Mapping

| Risk | Priority | AC | Component(s) | Test plan owner |
|------|----------|----|--------------|-----------------|
| R-01 Global seed fires on local serve path | **Critical** | AC-01, AC-06 | C4 | global-serve-seed.md |
| R-02 AC-06 sentinel green by reasoning not empirics | **Critical** | AC-06 | C4 | global-serve-seed.md |
| R-03 Seed write not atomic-no-clobber (clobbers operator config) | **Critical** | AC-05 | C1, C3, C4 | seed-write-primitive.md (+ C3/C4) |
| R-04 Locked surface hand-enumerated, drifts from A | **Critical** | AC-03, AC-04 | C2, C5 | per-slug-seed-renderer.md, seam-warn.md |
| R-05 Per-slug seed touches shared (a)≡(c) / wrong base | **Critical** | AC-02 | C3 | per-slug-seed-writer.md |
| R-06 A→B classification drift over time | High | AC-03 | C2, C5 | per-slug-seed-renderer.md, seam-warn.md |
| R-07 WARN raw-parse adds error path / alters resolution | High | AC-04 | C5 | seam-warn.md |
| R-08 WARN granularity / dedup-state scope wrong | High | AC-04 | C5 | seam-warn.md |
| R-09 Signature / match-arm ripple breaks existing tests | Med | (all) | C1, C3, C4 | seed-write-primitive.md (ripple audit) |
| R-10 Best-effort seed failure fails the command | Med | AC-05 | C3, C4 | per-slug-seed-writer.md, global-serve-seed.md |
| R-11 Dual (a) writers conflict | Low | AC-01, AC-05 | C4 | global-serve-seed.md |
| R-12 Field-less locks mis-render / panic | Med | AC-03 | C2 | per-slug-seed-renderer.md |
| R-13 Per-slug seed missed on State B | Med | AC-02 | C3 | per-slug-seed-writer.md |
| R-14 Seeded (b) body not resolver-loadable | Med | AC-02 | C2, C3 | per-slug-seed-renderer.md (round-trip) |

## 4. AC → Test Anchor (the six load-bearing proofs)

| AC | The load-bearing test | Owner |
|----|-----------------------|-------|
| **AC-01** | `serve`/seed with `http.enabled=true` AND **`base_dir = None`** writes (a) at `paths.data_dir.join("config.toml")` — proves the gate is `http.enabled`, not `base_dir` (the correction). | global-serve-seed.md |
| **AC-02** | `register <slug>` → `resolve_slug_config(slug)` round-trip reads the seeded (b) with **zero hand-placement**; (a)/(c) byte-unchanged. | per-slug-seed-writer.md |
| **AC-03** | **Flip test**: flip one key's `OverlayDisposition` → the rendered seed legend line flips (editable ↔ "managed globally"). Full-registry annotation coverage. | per-slug-seed-renderer.md |
| **AC-04** | **Flip test (WARN half)** + once-per-boot + key+slug naming + resolution-output equivalence with/without the WARN path. | seam-warn.md |
| **AC-05** | Pre-place operator content in (a) and (b) → byte-for-byte survival after serve + register; `AlreadyExists` is a silent no-op; no `path.exists()` precheck. | seed-write-primitive.md (+C3/C4) |
| **AC-06** | **Empirical zero-files sentinel**: local (`http.enabled=false`) file-count delta == 0, **WITH a negative control** showing delta > 0 on the `http.enabled=true` path. | global-serve-seed.md |

### The two "proven, not restated" centerpieces (must not be skipped — #3386 edge-case-skip lesson)

1. **A→B flip test (R-04/R-06, AC-03+AC-04)**: ONE flip of a key's `OverlayDisposition` must move
   BOTH the rendered seed annotation (C2) AND the WARN behavior (C5). This is the single proof that
   both derive from the registry at runtime, not a hand-list. Plan it in BOTH per-slug-seed-renderer.md
   and seam-warn.md; the implementation may share a flip harness but each behavior gets its own assertion.
2. **AC-06 empirical sentinel + negative control (R-01/R-02)**: count files before/after on the local
   path (delta == 0) AND on the container path (delta > 0). The negative control is mandatory — a
   sentinel that never detects a write is worthless (#4876: gate-integrity claims verified empirically,
   not structurally).

## 5. Integration Harness Plan (infra-001)

### Which existing suites apply

vnc-041 changes the daemon's **filesystem provisioning** (seeds two `config.toml` files) and adds a
`tracing::warn`. It introduces **no new MCP tool, no new tool parameter, no schema/storage change, and
no resolution/behavior change** (WARN-only, ADR-005; resolution UNCHANGED, ADR-004/002). Per the suite
selection table:

| Feature touches... | Suite | vnc-041 relevance |
|--------------------|-------|-------------------|
| Any change at all | `smoke` (~15) | **MANDATORY minimum gate** — proves the seeded global (a) on the http path does not break handshake/startup/tool discovery. |
| Any server tool logic / startup | `protocol` (13) | Run — confirms MCP handshake + graceful shutdown unaffected by the new serve-time seed write. |
| Schema or storage / startup-path changes | `lifecycle` (16) | Run — restart persistence path; confirms the global seed write on an http-enabled boot does not perturb store/restart flows. |

### What gaps exist (MCP-visible behavior NOT covered by existing suites)

**None that warrant a new infra-001 test.** The two genuinely new behaviors are:
- **File seeding** — observable on the filesystem, not through the MCP JSON-RPC interface. Fully covered
  by in-crate integration tests (register→resolve round-trip, file-count sentinel). infra-001 exercises
  the binary through MCP and does not assert on seeded files on disk.
- **The locked-key WARN** — emitted to `tracing` (stderr logs), not surfaced in any MCP response. Not
  observable through the JSON-RPC interface. Covered by in-crate `traced_test` assertions (C5).

Both new behaviors are **filesystem / log** effects with **no MCP-visible surface** → per the "When NOT
to plan integration tests" guidance, unit/in-crate integration tests suffice; no new `suites/test_*.py`
additions are planned.

### New infra-001 tests to add (Stage 3c)

**None.** Rationale recorded above. Stage 3c runs the smoke gate + `protocol` + `lifecycle` as a
**no-regression net** (the global seed now fires on the http-enabled boot the harness uses — confirm it
does not perturb handshake, tool discovery, or restart persistence). If any of these suites fail, apply
the USAGE-PROTOCOL.md triage: a failure traceable to the new serve-time seed write is feature-caused
(fix); a pre-existing/unrelated failure is `xfail` + GH Issue (never fixed in this feature's PR).

### Stage 3c run commands

```bash
cargo build --release
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60          # MANDATORY gate
python -m pytest suites/test_protocol.py -v --timeout=60   # startup/handshake regression net
python -m pytest suites/test_lifecycle.py -v --timeout=60  # restart-persistence regression net
```

## 6. Cross-Component Test Dependencies

- **C2 ↔ C5 share the A→B registry** (`PER_SLUG_CONFIG_CLASSIFICATION` / `is_per_slug_overlayable`). The
  flip test is the shared binding proof — plan it for both, optionally sharing one flip harness.
- **C1 underpins C3 and C4** — both seed writers MUST route through the extracted `write_if_absent`
  no-clobber primitive. C1's no-clobber + best-effort tests are the foundation; C3/C4 add only the
  call-site-specific proofs (correct path, correct branch, byte-survival end-to-end).
- **C3 ↔ resolver round-trip** — the highest-value integration assertion (AC-02): `register` then
  `resolve_slug_config` with no hand-placement. Depends on `per_slug_data_dir` being the single join site.
- **C2 ↔ C3 ↔ C5 round-trip (R-14)** — render → seed → resolve a pristine (b): parses, resolves clean,
  emits NO WARN (all locked keys commented out). One test threads all three.

## 7. Regression / Ripple Audit (R-09, SR-08 — confirm during 3a, verify 3c)

The C1 extraction (`write_if_absent` from `write_default_config_if_absent`) and the additive C3/C4 call
sites MUST NOT break:
- The four existing `write_default_config_if_absent` tests (`config.rs:11262–11346`):
  creates-when-absent, no-overwrite-no-force, force-overwrites, succeeds-even-if-write-fails. **Note for
  delivery**: the existing `force=true` arm uses `fs::write` (config.rs:4858), the `force=false` arm uses
  `create_new` (4873). The C1 extraction must preserve BOTH behaviors — `write_if_absent` is the
  `force=false` no-clobber body; `force=true` is NOT a seed and keeps its overwrite semantics. The
  `force=true` (overwrite) test MUST still pass.
- `main_tests.rs` `Command::Version` arm and any `register` call-site tests — additive call sites only,
  no signature/variant shape change (R-09).

## 8. Test Inventory (count target)

| Component | Unit | Integration | Total (approx) |
|-----------|------|-------------|----------------|
| C1 seed-write-primitive | 6 | — | 6 |
| C2 per-slug-seed-renderer | 8 | 1 (round-trip) | 9 |
| C3 per-slug-seed-writer | 4 | 4 | 8 |
| C4 global-serve-seed | 4 | 4 | 8 |
| C5 seam-warn | 9 | 1 | 10 |
| **Total** | **31** | **10** | **~41** |

Plus the 4 pre-existing config-write tests (must stay green) and the infra-001 smoke+protocol+lifecycle
regression net.

## 9. Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_search` (category=decision, topic=vnc-041) →
  surfaced ADR-001..005 (#5235/#5236/#5237/#5238/#5239) — all consumed as ground truth for the gate
  corrections (http.enabled not base_dir; create_new not fs::write; registry-derived WARN). Also #3386
  (edge-case-skip lesson → flip test + sentinel are mandatory, not bottom-of-plan optional), #4876
  (gate-integrity empirical → AC-06 negative control).
- Stored: nothing novel at plan-design time — patterns are feature-specific reuses of established
  conventions (`traced_test`/`logs_contain`, `TempBase`, `Fixture`, `create_new` no-clobber). Stage 3c
  may store a reusable "flip-an-OverlayDisposition harness" pattern if it generalizes.
