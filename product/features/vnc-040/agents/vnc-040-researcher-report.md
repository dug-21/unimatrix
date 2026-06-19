# vnc-040-researcher report

## Deliverable
SCOPE.md at `product/features/vnc-040/SCOPE.md` — per-slug config overlay resolution (C6 / Feature A of #785).

## Key findings
- The crt-056 seam threads **9 params** into `build_project_server` (issue says "8" — it lumps
  `boosted_categories` with `categories`). Verdict table in SCOPE covers all 9.
- `merge_configs` (`config.rs:3825`) is an existing per-key merge — C6 reuses it, doesn't invent one.
- "ADR-003 replace semantics" = dsn-001 **#2286** (global→project merge), NOT crt-056 ADR-003 (#5166).
  No conflict: C6 is a third precedence layer using the same field-level replace discipline.
- Hash-pin global-wins carve-out (#4655/#4649) must be preserved inside the per-slug merge — reinforces
  the model invariant.
- Per-slug file location: `{base_dir}/{slug}/config.toml`, sibling of the path-hash dir; reuses
  `load_single_config` + `validate_config`.
- A is decoupled from the vnc-038 register path — recommend **A-only** breadth; B (seeding) stays out.

## Open questions for human (in SCOPE)
1. Confirm per-slug file path/name (`{slug}/config.toml`).
2. `nli_top_k`/`nli_enabled` overlay verdict (recommend overlayable, confirm not model-coupled).
3. `inference_config` partial overlay (weights overlayable, pins global-wins).
4. Breadth: A-only vs A+B (recommend A-only).
5. `adapt_service` stays out of scope (confirm).
6. Per-slug file validated independently before merge (recommend yes).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search -- surfaced #5148 (C6 capability),
  #5165 (crt-056 ADR-002 seam), #2286 (dsn-001 ADR-003 replace merge), #4655/#4649 (hash-pin
  global-wins), #5079 (vnc-038 restart-applies routing). All applied in SCOPE.
- Stored: nothing novel to store -- findings are feature-specific (the 9-field verdict, file path,
  breadth call) and live in SCOPE.md; the generalizable pattern (per-key merge + hash-pin carve-out)
  is already captured in #4655/#2286.

---

## REVISION (2026-06-19, human gate) — second model invariant + open-question resolution

### Correction applied
The original verdict table + model invariant covered only `nli_handle` + `rayon_pool`. The live
`build_project_server` signature (`http_provision.rs:135`) threads a SECOND model handle the scope
omitted: `embed_handle: &Arc<EmbedServiceHandle>` — a PRE-EXISTING param positioned BEFORE the
crt-056 block (lines 138-155), NOT among the 9. Verified the position directly.

Changes (targeted; original sections preserved where correct):
1. Verdict table — `embed_handle` added as input #0 (GLOBAL — locked, hard invariant); whole
   `[embedding]` section locked global. Reframed as "10 relevant call-site inputs."
2. New Background subsection — "[embedding] section locked GLOBAL": sha256 carve-out is necessary
   but NOT sufficient; with no global pin, a per-slug `[embedding].model` causes a config-vs-handle
   divergence (served handle stays global by construction, but merged config DESCRIBES a different
   model). Only defused today by luck — vector index uses `VectorConfig::default()`
   (`http_provision.rs:182`), not config-driven dims. Locking the whole section closes it by
   construction, symmetric with transport.
3. Goal 3, Non-Goals, Proposed Approach, Constraints — extended to both models + the section lock.
4. AC-04 — asserts embedding-model parity EXPLICITLY (one embedding model at N≥2; per-slug
   `[embedding]` override neither loads nor describes a second), named specifically like the NLI one.
5. AC-07 — closed checklist reframed to all 10 inputs + `[embedding]` section; ties to crt-056
   AC-1's silent-drop-prevention rationale.

### Open questions — all six RESOLVED (human approved every recommendation)
Converted the Open Questions section to a resolution ledger: file path
`{base_dir}/{slug}/config.toml` CONFIRMED (same path Feature B seeds); nli_top_k/nli_enabled
overlayable; inference weights overlay / pins global; A-only (B is tracked follow-up, out of scope);
adapt_service out of scope; per-file validate-before-merge = AC-08. ADR-003 = dsn-001 #2286
field-level replace restated as settled (not re-litigated).

### Knowledge Stewardship (revision)
- Queried: context_briefing -- surfaced vnc-038/vnc-034 ADRs + hash-pin context; no entry on the
  config-vs-handle divergence pattern (global handle + overlayable config section describing it).
- Stored: pattern via /uni-store-pattern -- "When a handle is global/locked, lock the WHOLE config
  section that describes it, not just its sha256 pin" (config-vs-handle divergence). Generalizes
  beyond vnc-040.
