# 1036-vision-guardian report

Output: product/features/1036/ALIGNMENT-REPORT.md
Result: PASS 3, WARN 2, VARIANCE 1 (2 items: V-1, V-2), FAIL 0.

Variances for the human:
- V-1 (RTS R-01/OI-1): under D-023, an existing world-readable remote.json is never tightened. Credstore forces 0600 today, so this loosens current behaviour. A trust-set file whose mode drifted causes a "run init" loop that init cannot break.
- V-2 (RTS R-03/OI-2): the D-010/AC-3 rule to follow symlinks also covers repo-controlled project files. A cloned repo can then steer init's writes outside the project.

WARNs:
- Arch §9 Q1: the D-019 register text conflicts with SCOPE In-3/AC-1 on ~/.claude.json projects[root]. The design follows SCOPE. D-019 needs a one-line clarification.
- The RTS has stale refs: #5951/#5952, OI-3, R-18 "FOREIGN/unknown", "§9.1 recommendation".

Unimatrix entries consulted: #5836, #5681, #5937, #5950, #5954, #5803, #3337

## Knowledge Stewardship
- Queried: context_search for vision alignment patterns. Found #3337, which applies to the stale RTS strings.
- Stored: nothing new. V-1 and V-2 are specific to this feature. The stale-RTS finding is covered by #3337.
