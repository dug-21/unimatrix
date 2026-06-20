# nan-020 Architect Report — agent nan-020-agent-1-architect

## Deliverables
- `product/features/nan-020/architecture/ARCHITECTURE.md`
- `product/features/nan-020/architecture/ADR-001-extend-in-place-exit-code-contract.md` (Unimatrix #5249)
- `product/features/nan-020/architecture/ADR-002-in-test-bundle-emission-host-container-split.md` (Unimatrix #5250)
- `product/features/nan-020/architecture/ADR-003-executable-claim-boundary.md` (Unimatrix #5251)
- `product/features/nan-020/architecture/ADR-004-uni-docs-remit-widen.md` (Unimatrix #5252)

## Key decisions
1. Extend docker-http-posture-smoke.sh in place; new bundle-attach failures fold into the existing fail() exit-1 with distinct messages; run_smoke_gate UNCHANGED; Gates 1-4 are the structural regression guard (SR-04/SR-08, C-3).
2. In-test bundle emission with a host/container runtime split: emit (Rust client-bundle) in-container, consume (JS init --bundle) on the host, because the distroless image ships no JS and the host is the operator surrogate (SR-01/SR-03; A3 corrected).
3. Executable-claim vs narrative-prose operational contract; tested set is exactly the one canonical attach chain (SR-06, C-3, D-3).
4. uni-docs remit widened to all of docs/, blast-radius-scoped, authorship only (SR-05/SR-07, C-4/C-5).

## Verified facts
- client-bundle: Command::ClientBundle { slug }, sync pre-tokio (main.rs:293,437). A1 CONFIRMED.
- Image is distroless, Rust binary only, NO node/JS (Dockerfile:110,165). A3 CORRECTED.
- Node v24 present on host; packages/unimatrix in checkout.
- --slug is RETIRED on the init bundle path (init.js:353) -> OQ-A.

## Open questions
- OQ-A (spec/uni-docs, load-bearing): AC-02 says --bundle <blob> (+ --slug) but code retires --slug on the bundle path; docs+doc-test MUST use --bundle <blob> with no --slug.
- OQ-B (spec): README has 3 bundle phrasings (line 123 broken, 130, 587); rewrite must converge all on init --bundle <blob>; enumerate every occurrence.
- OQ-C (tester): Gate 6 init writes ~/.unimatrix/<hash>/ + a working tree; use throwaway --project-dir + HOME-isolated credstore for hermetic CI.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned nan-019 exit-code/run-marker ADRs (#5183 verify-by-name gate, #5202 IMAGE= bytes, #5185 store-grew gate, #5208 cross-runner lesson) and the N5 deployable-as-released capability (#5163). Applied all to the extend-in-place exit-code design.
- Stored: #5249 ADR-001, #5250 ADR-002, #5251 ADR-003, #5252 ADR-004 via context_store (category decision, topic nan-020). No supersession (nan-019/nan-005 ADRs extended, not invalidated). No typed edges asserted -- none meet the traversal-necessity bar at authoring; intra-feature Prerequisite spine left for retro per convention.
