# Agent Report: vnc-041-agent-0-scope-risk

**Mode:** scope-risk
**Output:** product/features/vnc-041/SCOPE-RISK-ASSESSMENT.md (9 risks, under 100 lines)

## Risk Summary
- High severity: SR-01, SR-02, SR-04, SR-05, SR-07 (5)
- Med severity: SR-03, SR-06, SR-08, SR-09 (4)
- Highest priority (High sev × High likelihood): SR-02

## Top 3 for Architect Attention
1. **SR-02** — WARN/annotation "locked surface" must derive from A's `is_per_slug_overlayable` at runtime, not a hand-list (`permissive` has no field; transport=construction-locked; pins=merge-locked, per #5211).
2. **SR-05** — The shared (a)≡(c) path-hash `config.toml` is written by two paths in the `register` flow; B must write ONLY the distinct per-slug file (b), never (a)/(c).
3. **SR-04** — Global seed must not regress the local/single-project majority (#5206 cites #4583 silent-fallback bug); confine to container `serve` structurally and assert local writes zero files (AC-06).

## Knowledge Stewardship
- Queried: /uni-knowledge-search (context_search) for config-seeding lessons, classification-drift patterns, byte-for-byte regression — found #5206 (vnc-040 ADR-002, names this feature's R-13 residual + single-project blast radius), #5211 (GlobalLocked merge-vs-construction split, permissive has no field), #4567 (write_default_config_if_absent create_new no-clobber + variant ripple), #665 (File::create TOCTOU). All four directly cited in the assessment.
- Stored: nothing novel — the recurring traps here are already captured as feature patterns (#5211, #5206) and a config-write lesson (#4567); no new cross-2+-feature pattern emerged that those don't cover.
