# Scope Risk Assessment: vnc-027

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Node FNF write-then-destroy semantics: `socket.destroy()` can drop unflushed buffered data, silently losing frames the Rust `fire_and_forget` (write, no read, disconnect) never loses. Fail-open masks the loss. | High | Med | Architect must specify flush-before-destroy mechanics (`socket.end()` + drain vs destroy) and a parity test that detects truncated frames server-side. Unimatrix #3448 documents expected broken-pipe/early-EOF noise on this path. |
| SR-02 | Size budget: client at 99,997/100,000 bytes (3 bytes headroom). A `format_injection` JS port is the single largest addition risk; vnc-026 already hit Gate-3b rework on this exact gate (Unimatrix #4780). | High | High | AC-09 (gate redefinition) must be the first merge-ordered change. Spec writer should make OQ2 server-side-preformatted the default position — it removes the budget driver entirely. |
| SR-03 | OQ2 server-side preformatted responses require an additive wire-contract change against the frozen F1 contract; any non-additive slip breaks existing parity fixtures and ts-rs bindings. | Med | Med | Architect confirms the wire mechanism as `skip_serializing_if` optional only; spec adds an explicit AC that Rust-hook fixtures pass byte-unchanged. |
| SR-04 | Known open parity divergence (lone-surrogate handling, Unimatrix #4788) is inherited by the UDS parity layer — "byte-identical" AC-03 may be unattainable without resolving or formally excepting it. | Med | High | Spec writer: enumerate accepted divergences in the parity AC, or scope the lone-surrogate fix in. Do not leave the parity bar ambiguous. |
| SR-05 | Sync UDS round-trip in async Node within a short-lived hook process: partial frame reads, premature `process.exit(0)` before socket drain, and the 20 ms p95 budget (AC-05) interact badly. | Med | Med | Architect specifies the read-loop/exit sequencing pattern explicitly; latency measured with the same protocol as F3 AC-13. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Hook-set reduction creates deliberate divergence from the parity oracle: the Rust hook still sends PreToolUse observation and SubagentStop. The parity corpus and "round-trip parity" goal partially contradict the reduction goal. | Med | High | Spec must split the parity bar: transport/framing parity is full; event-set parity is explicitly not a goal. The parity corpus excludes retired events. |
| SR-07 | Dogfooding switchover (OQ4) in-feature: fail-open by design means a silent event-drop bug costs this repo's knowledge capture for the entire soak window before anyone notices (Unimatrix #4473: warn-continue masks failure paths). | Med | Med | Recommend a cheap drop-detector for the soak: compare daemon-side event counts or queue residue before/after switchover; define a rollback trigger. |
| SR-08 | Carry-item FR-16 (offset delete rekeyed to TaskCompleted/age-prune) is a behavior change to delta streaming shared with the HTTP path — scope creep risk of regressing F3 remote behavior inside a "local transport" feature. | Med | Med | Spec writes explicit ACs for the HTTP path's unchanged externally visible behavior; keep the change minimal (key change only, no streaming redesign). |
| SR-09 | "SubagentStop optional" semantics (OQ3) span installer territory owned by F5 — a settings key is user-visible config surface. Boundary with F5 init/installer UX is thin. | Low | Med | Adopt the uni-zero recommendation (settings key, default-off) and state in the spec that F5 owns any UX around it. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-10 | Shared queue replayed over "whichever transport the next spawn selects": frames enqueued under UDS replayed over HTTP (or vice versa) must be accepted identically — auth context, content-type, and listener-vs-HTTP ingest differences could reject replayed frames. | High | Med | Architect verifies HookRequest JSON frames are genuinely transport-agnostic at both ingest points; spec adds a cross-transport replay AC. |
| SR-11 | PreCompact double-prepend across mixed clients: server builds the block from the F2 buffer; its empty-buffer guard only protects clients that never stream deltas. A TS client that streamed deltas then a Rust hook firing PreCompact (mixed install) yields client-prepend + server block. | Med | Low | Architect documents the mixed-client PreCompact matrix; AC-06 covers TS-only — add a stated assumption that one project uses one client. |
| SR-12 | Socket-path derivation parity: TS `projectHash` must match the Rust daemon's `{project_hash}` in all cases, including worktrees (OQ5 unanswered: does hook `cwd` for worktree-isolated subagents carry the worktree path?). Mismatch = silent local-mode failure (fail-open enqueue forever). | High | Med | Run the OQ5 stderr dump at design time as scoped; add a hash-parity fixture (same repo, main + worktree) to the corpus. Note #679 just fixed worktree-root resolution — fresh, fragile ground. |
| SR-13 | Transport selection replaces the terminal `missing`-config breadcrumb: users with no remote config AND no running daemon now enqueue indefinitely instead of breadcrumbing. Unbounded queue growth / stale-frame replay risk. | Med | Med | Confirm queue age-prune covers this; spec states the no-daemon UX (stderr one-liner per AC-04) and queue retention bound. |

## Assumptions

1. **Local daemon presence** (SCOPE Goals 1, AC-02): local mode assumes a running daemon at the derived socket path. If most local users run without one, the default-to-UDS change converts a clear config error into silent queueing. Invalidates the "replace breadcrumb with local mode" framing.
2. **projectHash equivalence** (SCOPE Background — TS client): assumes the F3 gitdir-resolution port produces hashes identical to Rust in every layout (worktrees, symlinks). If wrong, local mode never connects — see SR-12.
3. **Server-side formatting is wire-feasible additively** (OQ2): assumes the UDS HookResponse can carry preformatted text without breaking the frozen F1 contract. If wrong, the format_injection port returns and SR-02 escalates.
4. **C-04 redefinition is approved as stated** (Goals 4a): all client additions assume the 100 KB comment-stripped / 160 KB raw gate. A different human decision re-blocks the feature.
5. **Parity oracle stability** (Background — Rust side): assumes `hook.rs`/`wire.rs` stay frozen through F4a (Constraints say so); any concurrent Rust-side change invalidates goldens mid-feature.

## Design Recommendations

1. **Resolve OQ2 first, in architecture** — it determines the largest risk cluster (SR-02, SR-03). Server-side preformatted is strongly supported by history (vnc-025 ADR-005 shared-core parity-by-construction, Unimatrix #4743) and by the F6 dead-weight argument.
2. **Define the parity bar precisely** (SR-04, SR-06): which events, which accepted divergences, transport-parity vs event-set-parity. Ambiguity here was the vnc-026 rework driver.
3. **Specify Node socket lifecycle as a contract** (SR-01, SR-05): flush/drain-before-exit sequencing for FNF and sync paths, with server-side truncation detection in the test plan.
4. **Settle SR-12 empirically at design time**: the OQ5 stderr dump plus a TS-vs-Rust hash fixture is cheap and eliminates the silent-failure mode.
5. **Sequence merges**: AC-09 size gate → transport → hook-set reduction → dogfood switchover with a drop-detector (SR-07).
