> **DEFERRED — not part of vnc-045 delivery; carried to the future `protected_tags` feature.** (Deprecated in Unimatrix 2026-07-07 under the vnc-045 scope reduction; reasoning preserved here for the future feature. No per-slug config threading, no `ProtectedTagsConfig` type, and no server-state field ship in vnc-045 — building inert threading before the consuming feature exists is the `::default()`-rots trap this scope reduction voids.)

## ADR-005: `protected_tags` per-slug threading — five sites, all paths honored, behavioral per-path verification

### Context
`protected_tags` is per-slug (`PerSlugOverlayable`, SD-13 LOCKED): the containerized server must run separate projects with different tag settings from one instance. Adding it requires real extension across FIVE coupled config sites (SCOPE item 5), not a free additive scalar. SR-06 (High/High) is the top scope risk: there is strong recurrence history of one threading site being missed SILENTLY (#3216, #2398 dsn-001 gaps), and the build-enforced classification drift-guard (slug_config_classification_tests.rs:397) catches an ABSENT key, NOT INCORRECT threading (#5427 string-count tests blind to threading). Worse, code inspection of the three `UnimatrixServer::new` construction sites shows `build_project_server` (http_provision.rs:261) sets NO server-state config field — it does not even set the existing `store_config`. Config is wired post-construction ONLY in the daemon block (main.rs:982) and stdio block (main.rs:1701), both from GLOBAL config. A field added naively to just those two blocks would leave the **per-slug HTTP path** on `ProtectedTagsConfig::default()` — the exact opposite of SD-13, and silent (#5269 daemon-vs-per-slug divergence).

### Decision
**Five-site checklist (all mandatory):**
(a) New nested config type `ProtectedTagsConfig { rules: Vec<ProtectedTagRule> }` + field on `UnimatrixConfig` (config.rs:72); `min_trust_level` deserialized from string.
(b) New `merge_configs` arm (config.rs:3826) — **REPLACE** the rules list (ADR-006).
(c) `validate_config` extension (config.rs:3397 per-file AND http_provision.rs:381 post-merge, per vnc-040 ADR-003 #5199) — prefix well-formedness, non-empty `allowed_values`, no duplicate prefixes.
(d) `PER_SLUG_CONFIG_CLASSIFICATION` entry (config.rs:4447): `{ key:"protected_tags", disposition: PerSlugOverlayable }` — build-enforced, LOCKED.
(e) Server-state threading: `pub protected_tags: Arc<ProtectedTagsConfig>` on `UnimatrixServer` (beside `store_config` server.rs:267; init default in `new()`), populated on **all three** construction paths.

**Daemon-path behavior — deliberate decision: honor the policy on every path, none inert.**
- Daemon / local-UDS (main.rs:954, wired :982) → `Arc::new(config.protected_tags.clone())` from GLOBAL config. Single project → global IS its policy. Honored.
- Stdio (main.rs:1673, wired :1701) → same, from GLOBAL config. Honored.
- Per-slug HTTP → **add a `protected_tags` param to `build_project_server` (http_provision.rs:136-160)**, derive it per-slug from the resolved config (`r`/`resolved`, main.rs:1170-1202), and set it on the server BEFORE `ProjectServerInput` is returned (http_provision.rs:275) — because the post-loop code (main.rs:1229+) only reads accessors and never sets per-server config. Honored per-slug.

This is stronger than SCOPE assumption A2 (which tolerated daemon-path inertness): the same config key behaving differently by launch mode is a latent operator surprise, and the wiring is symmetric (one populate line per path). Making it inert nowhere eliminates the divergence rather than documenting it as accepted.

**Verification is behavioral per-path, not source-assertion counting.** For each of the three construction paths, a test configures a `protected_tags` policy and asserts it reaches `self.protected_tags` in the handler and actually rejects an out-of-vocabulary value (`delivery:provn`). Two slugs on one instance are shown to carry DIFFERENT live policies (AC-08). The classification drift-guard (absent-key only) is necessary but insufficient; the behavioral matrix is what catches incorrect threading.

### Consequences
- Easier: SD-13 satisfied on the containerized multi-project target; the highest-likelihood failure mode (silent default on the per-slug path) is closed at the threading seam, not left to a source-count test.
- Easier: hygiene is available in every deployment mode with consistent semantics.
- New obligation (intended forcing function): the classification entry means a future config field crossing the seam requires a classification entry, and the behavioral matrix must add a row per new construction path.
- Cost: `build_project_server`'s signature grows by one param; three populate sites to keep in sync — mitigated by the behavioral per-path test.
- Cross-references ADR-006 (merge replace), ADR-003 (the policy consumed here), vnc-040 ADR-001/003/004 (#5209/#5199/#5217), SR-06.
