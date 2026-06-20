# nan-020-agent-2-spec — Agent Report

## Deliverable
`product/features/nan-020/specification/SPECIFICATION.md`

## Summary
Refined locked SCOPE (AC-01..AC-08, D-1/D-2/D-3, C-1..C-6) into 21 testable functional requirements, 8 NFRs, and an AC table with concrete verification methods. Grounded against real surfaces: `docs/client-setup.md`, README remote-mode section, `docker-http-posture-smoke.sh`, `release-gate-lib.sh`, `uni-docs.md`, and the shipped CLI (`unimatrix.js`, `init.js`, server `main.rs`).

## Key decisions / interpretations
- **Executable claim vs narrative prose** given an operational definition + worked example tied to AC-03's concrete testable set (`client-bundle` emit → `init --bundle` → `POST /v1/{slug}/observe` 204 + per-slug store grew). Closes SR-06.
- **Blast radius** defined concretely as "surfaces a change touches" in both spec and the uni-docs remit requirements (FR-18). Closes SR-07.
- **Skip paths enumerated** (no Docker, binary absent, emit failed, route/flag absent) each requiring a distinct fatal exit that hard-fails the gate; preserves nan-019's 0/1/3/4 truth table. Closes SR-04/SR-08.
- **uni-docs edit fenced** to authorship-remit text only — no drift-checker/gate/trigger (Feature 2). FR-19. Closes SR-05.

## Open questions raised
- **OQ-A (refinement):** Shipped `init` RETIRES `--slug` on the bundle path (bundle encodes slug — `init.js`). Spec specifies docs as `init --bundle <blob>` WITHOUT `--slug`, refining SCOPE's parenthetical "(+ `--slug`)". As-shipped form is the testable truth.
- **OQ-B (SR-01):** Architect must confirm `client-bundle <slug>` is invocable inside the *shipped* image (path + exact name/signature) before AC-03 is locked to it. Highest-leverage de-risk.
- **OQ-C (SR-03):** Confirm production image + gate lane supply both runtimes (Rust `client-bundle`, JS `init`); keep the assertion sound.
- **OQ-D (D-2 caveat):** Confirm reuse of existing smoke boot config; split to sibling only if it genuinely diverges.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — nan-019 patterns (#5192, #5185), nan-005 uni-docs precedent (#1255), personal-cloud bundle-attach capability (#5151). Read-only tier; no storage.
