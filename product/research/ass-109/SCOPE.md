# ASS-109: One Node client for local and cloud — production install vs. this repo's dogfood

Issue: #1020

## Question
Do production local installs and this repo's dogfood setup both run on the single Node client we committed to, and what install procedure should each use? Is the Rust binary still acting as a client anywhere?

"Client" means what Claude Code (or another harness) invokes through its MCP definition and hook commands. The Node client talks to the Rust server binary.

## Why it matters
The personal-cloud goal requires local/cloud parity. To get it we committed to one Node client for both, and to retiring the Rust client path (hook.rs) once TS+UDS reached parity. Residual Rust client paths, or divergence between production installs and this repo, mean parity isn't real and cleanup attempts keep tangling. A prior cleanup attempt failed because these two scopes were mixed; this spike separates them before any fix is scoped.

## Background (verify, do not assume)
- The Node client was built under the personal-cloud goal. The local-development process against unreleased components is "dogfood" / "dogfooding".
- Production repos install via npm. Dogfooding applies only to this repository: here components (schema migrations, hook-config changes) get upgraded before a release exists, so the released npm package can lag the code.
- **Hypothesis:** dogfooding was set up and worked at one point, then regressed — likely when the Codex and OpenCode harnesses were added.

## Sources (the history is recorded; find it, don't reconstruct it)
- Unimatrix: search `dogfood`, Node/TS client, Rust client (hook.rs) retirement, per-harness wiring. Read ADRs with their correction chains; note superseded entries.
- `product/features/` design packages: personal-cloud client work (TS thin client, packaging, F1–F4, vnc-034) and harness additions (Codex, OpenCode / vnc-049, nan-023 install options).
- `product/research/`: ass-067/068 (TS thin client + packaging), ass-069, ass-106 (OpenCode).
- Git history of the dogfood scripts, `.mcp.json`, `.claude/settings.json`, `.codex/`, `opencode.json`, and the npm package's init/wire code.

## Bounded questions (priority order)
- **0. Regression timeline.** Last known-good dogfood setup (commit + design). The change(s) that broke it; was it the Codex/OpenCode addition? Prove or disprove the hypothesis.
- **1. Decision history.** Which ADRs and goal text commit to one Node client and Rust-client retirement, and what each committed to. Which are current vs. superseded.
- **2. Rust client usage.** Every place the Rust binary is still invoked as a client — MCP definitions, hook commands, init/wire output, scripts, docs, CI — per harness (Claude, Codex, OpenCode). Verdict: proven/disproven that Rust client usage is little or none.
- **3. Production local install.** What an npm install in a clean repo writes and where: project files vs. user/local surfaces, absolute vs. portable values, per harness. Is the result portable, and does it leave host-owned config alone?
- **4. This repo's dogfood setup.** What the current dogfood scripts and config do, how that differs from a production install, and why. Which differences exist only because we upgrade before release?
- **5. Pre-release upgrade path.** Can this repo use the production install procedure with locally packed artifacts (npm pack + repo-built binary) and no swap scripts? What breaks — schema version, rollback, hook config?
- **6. Per-harness local surfaces.** For each harness, which project-local and user-level config surfaces exist and how they merge with project files.

## Method
- **Breadth:** code + history (Unimatrix, design packages, git); external docs only for harness config surfaces (Q6)
- **Approach:** investigation; a throwaway clean-repo npm install is allowed for Q3
- **Confidence required:** evidence-backed (commits, file paths, entry IDs) for Q0–Q4; directional for Q5–Q6

## Output
`product/research/ass-109/ass-109-findings.md` containing:
- the regression timeline with commits;
- the proven/disproven verdict on Rust client usage;
- the install procedure for production local installs and the one for this repo, described separately;
- a gap list, each gap tagged `production` or `this-repo-only`, sized to scope a follow-up bugfix or feature.

## Constraints
- Research only. No edits to `crates/`, the npm package, tracked harness config, or dogfood scripts.
- The installer must never own host-repo policy (what is tracked or gitignored). Recommendations must use per-harness local overlays for machine-specific values.
- Do not touch the live database at `~/.unimatrix/`. Any install experiment runs in a scratch repo with its own data dir.
- Do not cite the closed prior attempt as a source of truth; derive findings from the recorded history above.
