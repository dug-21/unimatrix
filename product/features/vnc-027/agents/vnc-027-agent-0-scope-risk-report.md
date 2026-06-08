# Agent Report: vnc-027-agent-0-scope-risk

**Mode**: scope-risk
**Output**: `product/features/vnc-027/SCOPE-RISK-ASSESSMENT.md` (under 100 lines)

## Risk Summary

13 risks (SR-01..SR-13): 4 High severity (SR-01, SR-02, SR-10, SR-12), 8 Medium, 1 Low. 5 assumptions, 5 design recommendations.

## Top 3 for Architect/Spec Attention

1. **SR-02 + OQ2** — 3-byte size headroom; a `format_injection` JS port is the largest budget driver. Resolve OQ2 (server-side preformatted) first in architecture; AC-09 gate redefinition merges first. Evidence: Unimatrix #4780 (vnc-026 Gate-3b rework on this exact gate).
2. **SR-01** — Node `socket.destroy()` can drop unflushed FNF frames; fail-open hides the loss. Architect must specify flush/drain-before-exit lifecycle with server-side truncation detection. Evidence: Unimatrix #3448.
3. **SR-12** — TS/Rust `projectHash` parity in worktrees is unverified (OQ5); mismatch = silent never-connect with indefinite enqueue. Settle empirically at design time; #679 just touched this ground.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for lessons (hook client/UDS/gate rejections), risk patterns (parity/framing), vnc-026 rework outcomes, hook-set reduction — found #4780 (size gate rework), #4788/#4775 (open lone-surrogate parity divergence), #3448 (UDS FNF expected I/O errors), #4743 (shared-core parity by construction), #4473 (warn+continue masks failure paths). All informed risk severity/likelihood.
- Stored: entry #4800 "Fail-open hook clients mask event loss — pair every fail-open contract with a drop-detector before relying on the client" via /uni-store-pattern (recurs across vnc-017 and F3/F4a).
