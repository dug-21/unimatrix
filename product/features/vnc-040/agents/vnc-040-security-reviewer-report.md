# Security Review: vnc-040-security-reviewer

PR: #799 (branch `feature/vnc-040`) — Per-slug configuration overlay resolution (C6 / Feature A of #785)
Reviewer: uni-security-reviewer (fresh context)
Date: 2026-06-19

## Risk Level: low

## Summary
The per-slug config overlay is well-defended. Path traversal is structurally impossible
(`ProjectSlug` allowlist), the hash-pin global-wins and `permissive`/transport global-lock
controls hold by construction at the seam, and the post-merge cross-field re-validation closes
the #3905 invariant-bypass class. The reused `load_single_config` carries the 64 KiB cap and the
unix permission check unchanged. No new dependencies, no secrets. One non-blocking
classification-completeness gap (`knowledge.adaptive_categories`) and one pre-existing
permission-check nuance are noted below.

## Findings

### F1 — `knowledge.adaptive_categories` is a seam input read from `resolved` but absent from the canonical classification registry and its exhaustiveness guard
- **Severity**: low
- **Location**: `main.rs` per-slug loop (`r.knowledge.adaptive_categories.clone()`); `config.rs` `PER_SLUG_CONFIG_CLASSIFICATION`; `slug_config_classification_tests.rs` `EXPECTED_CLASSIFIED_KEYS`
- **Description**: The loop derives `slug_categories` from BOTH `r.knowledge.categories` and
  `r.knowledge.adaptive_categories`. `merge_configs` merges `adaptive_categories` project-wins.
  Yet `adaptive_categories` appears in neither `PER_SLUG_CONFIG_CLASSIFICATION` (ADR-004's
  "single canonical source of truth, every seam key appears exactly once") nor the
  `EXPECTED_CLASSIFIED_KEYS` exhaustiveness list. The drift-guard test
  (`test_classification_registry_exhaustive_vs_seam_field_set`) compares the registry against a
  HAND-MAINTAINED expected list, NOT against the live set of fields the loop reads from
  `resolved` — so the "closed set" is closed against a second hand-list, not against the seam.
  This is exactly the R-07/R-14 failure mode the design names ("a seam input absent from the
  checklist"; the multi-copy-divergence pattern) materializing a third time.
- **Security impact**: None. `adaptive_categories` is per-slug knowledge config in the same trust
  class as `categories`; behaving as PerSlugOverlayable is correct and safe. No control is
  bypassed. This is a classification-completeness / drift-guard-strength gap, not a vulnerability.
- **Recommendation**: Add `knowledge.adaptive_categories` to `PER_SLUG_CONFIG_CLASSIFICATION`
  (PerSlugOverlayable) and `EXPECTED_CLASSIFIED_KEYS`. Longer-term, derive the exhaustiveness
  baseline from the live seam reads rather than a second hand-kept list, so the guard cannot
  silently miss a future field.
- **Blocking**: no

### F2 — Permission check hard-rejects only world-writable; group-writable warns (pre-existing, reused unchanged)
- **Severity**: informational
- **Location**: `config.rs` `check_permissions` (`mode & 0o002` rejects; `mode & 0o020` warns)
- **Description**: The architecture/risk docs describe the control as `mode() & 0o022` rejection.
  The actual reused control hard-rejects world-writable (`0o002`) and only WARNS on
  group-writable (`0o020`). The per-slug file inherits the identical control as the global/project
  config files — this is consistent daemon-wide behavior, NOT a regression introduced by this PR.
- **Security impact**: A group-writable per-slug `config.toml` is accepted with a warning. Trust
  boundary is the same as every other config file the daemon reads; acceptable given the
  config dir is operator-controlled.
- **Recommendation**: None required for this PR. If the docs' `0o022`-reject framing is intended
  as the real posture, that is a separate, daemon-wide hardening decision out of scope for vnc-040.
- **Blocking**: no

## Verified Controls (held)

| Control | Verdict | Evidence |
|---------|---------|----------|
| Path traversal from slug | CLOSED by construction | `ProjectSlug::try_from` enforces `^[a-z0-9][a-z0-9-]{0,62}$`; `.`/`/`/`\`/`%` cannot pass; `base_dir.join(slug.as_str())` cannot escape |
| Hash-pin global-wins (`*_sha256`) | HELD | `merge_configs` inference arm: if global pin `is_some()`, project value ignored + `tracing::warn` on mismatch (both `embedding_model_sha256`, `nli_model_sha256`) |
| `permissive` global-locked | HELD by construction | computed from global `config.agents.default_trust` (main.rs:688); passed unchanged; loop never reads `resolved.agents.default_trust` |
| Transport/TLS/http global-locked | HELD by construction | loop reads ONLY instructions/inference/knowledge/observation from `resolved`; never transport fields |
| Single model in memory (no 2nd load) | HELD by construction | 3 handles `Arc::clone`d unconditionally outside the overlay branch; never sourced from `resolved` |
| Post-merge cross-field re-validation (#3905) | HELD | `validate_config(&merged, &path)` called after `merge_configs`, before return; sum-of-six + per-field range checks present in `validate_config` |
| 64 KiB size cap (DoS) | HELD | reused `load_single_config` (`CONFIG_MAX_BYTES = 65536`) exercised on per-slug path |
| TOML deserialization | SAFE | `toml::from_str` into deny-unknown-aware struct; malformed → `ServerError::Config`, fail-loud, no `.unwrap()` |
| Error handling | SAFE | every failure path → slug-named `ServerError::Config` at startup; no panic, no request-time fallback |
| `merge_configs` field-completeness (#4070) | SAFE | inference arm is a fully-enumerated struct literal (no `..default` spread); an unhandled added field is a compile error |
| No-file fallthrough | PRESERVED | `Cow::Borrowed(global)` returned on no file; `r == &config`; derivations byte-for-byte equal global |

## Blast Radius Assessment
Worst case if the overlay/merge had a subtle bug: confined to ONE slug's `ServiceLayer`, evaluated
at startup. A bad per-slug file fails the daemon LOUD (startup-fatal, no partial serve), so the
failure mode is safe (daemon refuses to start), never silent corruption. The change CANNOT:
(a) load a second model — the 3 handles are cloned outside the merge branch and never read from
`resolved`; (b) alter another slug or the global config — each slug resolves independently from
the immutable global; (c) escalate permission posture — `permissive` is the global flag, never
sourced from `resolved`; (d) change transport/TLS — never read at the seam; (e) bypass a hash pin —
global-wins inside `merge_configs`. The highest-value attack (model substitution via per-slug pin)
is defused by the global-wins carve-out. The only path-traversal vector (slug → filesystem path)
is structurally closed at the `ProjectSlug` parse edge.

## Regression Risk
Low. The visibility-only change (`load_single_config`/`merge_configs` made `pub`) does not alter
logic. The no-file / single-project / local-UDS majority path is preserved: `resolve_slug_config`
returns `Cow::Borrowed(&global)` with no merge or re-derivation, and the loop derives all values
from `r == &config`, equal to the daemon's own values byte-for-byte. The daemon's OWN server
(main.rs:935) still uses the global `server_instructions`; relocating the per-slug `instructions`
source to `resolved.server.instructions` does not touch it. `merge_configs` logic is byte-for-byte
unchanged. Tests assert `Arc::ptr_eq` on the no-file arm, converting the no-re-derivation guarantee
to machine-checked.

## Dependency Safety
No `Cargo.toml` / `Cargo.lock` changes in the diff (confirmed). No new dependencies. The
pre-existing transitive CVE RUSTSEC-2023-0071 (rsa via sqlx-mysql) is daemon-wide and NOT
introduced or touched by this PR — out of scope for this change.

## Secrets
None. No hardcoded credentials, tokens, or keys in the diff. The hash-pin fields hold SHA-256
digests (integrity pins), not secrets.

## PR Comments
- Posted 1 review comment on PR #799 (advisory, F1 + F2 + verified-controls summary)
- Blocking findings: no (review submitted as --comment, not --request-changes)

## Knowledge Stewardship
- Nothing novel to store. The recurring pattern (third config layer must re-validate the merged
  result; canonical classification must be machine-pinned to live seam reads, not a hand-list) is
  already captured by #3905 and the R-07/R-14 design entries. F1 is an instance of the already-known
  crt-031 multi-copy-divergence pattern, not a new cross-feature lesson.
