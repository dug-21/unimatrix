> **DEFERRED — not part of vnc-045 delivery; carried to the future `protected_tags` feature.** (Deprecated in Unimatrix 2026-07-07 under the vnc-045 scope reduction; reasoning preserved here for the future feature. There is no `protected_tags` config to merge in vnc-045.)

## ADR-006: `merge_configs` replaces the `protected_tags` rules list; no inheritance

### Context
`merge_configs` (config.rs:3826) is hand-written with no catch-all; each new field needs an explicit arm, and the overlay behavior (global → per-slug) must be chosen deliberately. SR-09 (Med): list replace-vs-merge for `protected_tags` has a security edge — a slug that MERGES its rules with the base config's rather than REPLACING could silently INHERIT base-config allow-list prefixes/values it never declared. For a per-slug value-hygiene policy that is supposed to express one project's vocabulary, silent inheritance of another scope's rules is a correctness and least-surprise hazard. The established per-slug discipline (dsn-001 #2286, vnc-040) is field-level replace with list fields replacing (not appending).

### Decision
The `protected_tags` `merge_configs` arm uses **whole-list REPLACE**, consistent with vnc-040's list-replace discipline: if a slug's `config.toml` declares a `[protected_tags]` section, its `rules` list fully replaces the global list; a slug's policy is exactly what it declares, with NO per-rule merge and NO inheritance of undeclared base prefixes. If a slug declares NO `[protected_tags]` section, the global list flows through unchanged (byte-for-byte fallthrough, vnc-040 ADR-002) — the slug inherits the global policy wholesale, which is the intended default, not a partial merge.

Two states only, no middle ground:
- slug section ABSENT → global policy (fallthrough).
- slug section PRESENT → slug policy verbatim (replace); the operator sees exactly the rules they wrote.

The classification drift-guard test (vnc-040 ADR-004 #5217) asserts this arm's behavior matches the `PerSlugOverlayable` disposition: build a global and a slug config differing only on `protected_tags`, run `merge_configs`, assert `merged.protected_tags == slug.protected_tags`.

### Consequences
- Easier: a slug's hygiene policy is self-contained and auditable — what you declare is what enforces; no phantom prefixes leak in from global.
- Easier: the drift-guard pins replace behavior; a regression to accidental merge fails the build (#5211 pattern — but note: `protected_tags` is genuinely merge-locked-by-arm here, an ordinary `PerSlugOverlayable` replace, not a construction-locked or hash-pinned key).
- Trade-off (accepted): a slug that wants "global rules PLUS one more" must restate the global rules in its own file. For a small hygiene policy this is acceptable and safer than silent inheritance; documented so operators expect replace.
- Cross-references ADR-005 (site (b)), SR-09.
