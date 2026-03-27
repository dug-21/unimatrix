## ADR-004: Atomic Write for profile-meta.json via Rename

### Context

`profile-meta.json` is written by `eval run` after scenario replay completes. If `eval run`
crashes after some result files are written but before the sidecar is flushed, the results
directory contains per-scenario JSON files but no `profile-meta.json`. When `eval report` is
subsequently run against this directory, it falls back to backward-compat mode (AC-11):
all profiles treated as `distribution_change = false`, Section 5 rendered as Zero-Regression
Check.

This fallback is intentional and correct for pre-nan-010 result directories. However, for a
partial run of a distribution-change feature, this silent fallback is misleading: the
candidate profile declared `distribution_change = true`, but the report renders a
Zero-Regression Check that may show false regressions. The operator has no indication that
gating mode was degraded.

A non-atomic write introduces a worse scenario: a partially written `profile-meta.json` that
exists on disk but contains truncated or corrupt JSON. `run_report` would parse this as a
malformed sidecar, log a WARN, and fall back to backward-compat — again silently degraded.

The standard mitigation for atomic file writes on POSIX systems is write-to-temp, then rename.
`fs::rename` is atomic on POSIX when source and destination are on the same filesystem. This
guarantees that the sidecar either fully exists (rename succeeded) or does not exist (rename
never reached). There is no intermediate corrupt state.

### Decision

`write_profile_meta` in `eval/runner/profile_meta.rs` writes `profile-meta.json` atomically:

1. Serialize `ProfileMetaFile` to JSON string.
2. Write the JSON to `{out}/profile-meta.json.tmp`.
3. `fs::rename("{out}/profile-meta.json.tmp", "{out}/profile-meta.json")`.
4. If rename fails (cross-device move, which cannot happen within a single output directory),
   fall back to `fs::copy` + `fs::remove_file`.

`eval report` reads only `profile-meta.json`, never `profile-meta.json.tmp`. A leftover
`.tmp` from a crashed run is ignored.

The write happens after all scenario replay completes in `run_eval_async`, immediately before
returning `Ok(())`. This means the sidecar is absent if the run fails for any reason before
completion. That matches the intended semantics: the sidecar is a run-completion artifact.

### Consequences

Easier:
- No corrupt intermediate state. Either the sidecar is complete and valid, or it is absent.
- `eval report` backward-compat fallback (AC-11) applies cleanly: absent = pre-nan-010 or
  incomplete run.
- No new error modes for `eval report` to handle.

Harder:
- `write_profile_meta` requires two filesystem operations (write + rename) instead of one.
- A crash between write-to-tmp and rename leaves a `.tmp` artifact in the output directory.
  This is cosmetically untidy but functionally harmless.
- On cross-device filesystems the rename fallback path (copy + remove) is not atomic. This
  is an acceptable edge case — `eval run` output directories are local filesystem paths.
