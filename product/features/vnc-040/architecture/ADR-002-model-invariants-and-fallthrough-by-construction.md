## ADR-002: Both model invariants and byte-for-byte fallthrough hold BY CONSTRUCTION, not by test

### Context
vnc-040 must preserve crt-056 AC-2 — exactly one NLI model and exactly one embedding model in
memory at N≥2 slugs — while adding a per-slug config overlay. Two scope risks make this load-bearing:

- **SR-07:** the 6 overlayable `Arc`s derive from the merged config; fields 0–2
  (`embed_handle`, `rayon_pool`, `nli_handle`) must NEVER source from it. A merge that
  accidentally rebuilt a handle would load a second model and break AC-2.
- **SR-03 (necessary-not-sufficient):** even with handles cloned correctly, a per-slug file could
  set an embedding **descriptor** key, so the MERGED CONFIG would *describe* a model the served
  handle is not — a config-vs-handle divergence (Unimatrix #5196, #4655). The sha256 pin
  carve-out alone does not stop a `[embedding].model`/`dimensions` key where no pin is set.
- **SR-04:** any deviation from byte-for-byte fallthrough silently changes behavior for the
  single-project / local-UDS majority (#4583 shipped a silent-fallback config bug).

### Decision
**1. Three global handles cloned OUTSIDE any overlay branch.** In the per-slug loop, fields 0
(`embed_handle`), 1 (`rayon_pool`), 2 (`nli_handle`) are `Arc::clone`d from the daemon's single
loaded handles **unconditionally**, before/independent of `resolve_slug_config`, and passed into
`build_project_server` as those clones. They are never read from the merged config on any path.
No code path loads a second model — AC-2 holds by construction (extends ADR-002 crt-056's
"Arc::clone, never rebuild" to the embedding handle).

**2. Embedding descriptor locked global-wins.** Code inspection confirms there is **no separate
`[embedding]` section** in the current `UnimatrixConfig`; the only embedding-model descriptor is
`inference.embedding_model_sha256` (`config.rs:515`), which `merge_configs` ALREADY makes
global-wins (#4655, `config.rs:3905-3920`). vnc-040 adds **no new embedding descriptor field**
(scope Non-Goal). Therefore the merged config cannot describe a different embedding model — the
"whole `[embedding]` section locked global" requirement resolves to: pin stays global-wins +
no new descriptor knob. **Forward guard (#5196):** any future `[embedding].model`/`.dimensions`
field MUST be added global-wins, symmetric with transport — recorded so it is not silently
violated.

**3. `nli_top_k`/`nli_enabled` overlayable** (SR-08, OQ-2): runtime inference params that tune
how the shared `nli_handle` is queried, NOT model identity — safe to overlay.

**4. Fallthrough is structural AND machine-checked via `Arc::ptr_eq`.** The no-file arm returns
the global config and MUST reuse the daemon's already-built parity `Arc`s directly (the same
clones crt-056 passes today) — no merge runs, nothing is re-derived. The guarantee is asserted as
**pointer equality, not value equality**: the test asserts `Arc::ptr_eq` between the daemon's
handle and the per-slug clone for the 3 global handles (`embed_handle` / `nli_handle` /
`rayon_pool`) — exactly the assertion crt-056 AC-2 already uses. This converts "no re-derivation"
from a review-only property into a machine-checked one: a regression that rebuilt or re-derived
any of the three fails `Arc::ptr_eq` even if the rebuilt value compares equal. AC-02 thus becomes
a pointer-equality-grade regression sentinel: no-file ⇒ identical to the pre-vnc-040 crt-056 path
across every call-site input.

### Consequences
- **Easier:** AC-04 (both one-model invariants) and AC-02 (fallthrough) are guaranteed by the
  shape of the code, not contingent on a test catching a regression — closes the SR-01-class gap
  where a test could miss a cross-field hole.
- **Easier:** the single-project majority's blast radius is eliminated by construction — no merge
  on the common path.
- **Constraint recorded:** the embedding lock depends on the per-slug vector index staying
  `VectorConfig::default()` (A2). If dims become config-driven later, an explicit `[embedding]`
  section lock must be added — flagged for that future feature.
- **Cost:** the verdict is a CLOSED per-call-site-input checklist (AC-07, ARCHITECTURE §9) — every
  argument `build_project_server` takes gets a row, not "all 9 params" shorthand. The
  pre-existing `embed_handle`, and the later-noticed `permissive` and `instructions`, are exactly
  the silent-drops the checklist exists to catch (#5196): the framing is "no call-site input is
  absent," not a count.
- Depends on ADR-001 (the resolution helper); the post-merge validation is ADR-003.
