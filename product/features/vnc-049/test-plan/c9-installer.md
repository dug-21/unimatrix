# Test Plan — C9 Installer OpenCode Branch (JS)

`packages/unimatrix/lib/init.js` (detection branch) + new `packages/unimatrix/lib/opencode-install.js`
(non-clobber provisioning logic). Reuses the nan-004 (#1201) prefix-match non-clobber merge
principle (surface differs: plugin dir + `.opencode/package.json` dep + `plugin:[]` array).

Risks owned: **R-08, R-09** (primary), **R-15** (installer input). AC: AC-05 (a/b/c/d), NFR-03, NFR-06.

Test surface: JS `node:test` (extend the existing installer test files — do not fork):
`packages/unimatrix/test/init.test.js`, `init-integration.test.js`, and
`merge-settings.test.js` (the nan-004 prefix-match / non-clobber / idempotence home,
`packages/unimatrix/lib/merge-settings.js` @124/@201). Reuse the prefix-match identification
approach from `merge-settings.js` for the plugin-array/dep surface. Run against a temp fixture
OpenCode repo (a real `opencode.json` + `.opencode/`). The retrieval "still returns" leg (AC-05c)
crosses into infra-001 via MCP `context_*` after provisioning.

Existing tests to keep green (regression sentinel): `writeMcpJson` non-clobber tests in
`init.test.js` (`test_preserves_existing_servers` :109, `test_preserves_nested_env_args_in_other_servers`
:151); `merge-settings.test.js` `test_merge_idempotent_round_trip` (:173),
`test_each_event_has_exactly_one_unimatrix_entry` (:199), `test_merge_preserves_extra_top_level_keys`
(:164). New opencode-install tests mirror these patterns for the `.opencode/` surface.

## Unit / integration expectations

### Detection (FR-09, AC-05a)
- `test_installer_detects_opencode_by_opencode_json` — presence of `opencode.json` triggers the
  OpenCode branch.
- `test_installer_detects_opencode_by_dot_opencode_dir` — presence of `.opencode/` triggers it.
- `test_installer_no_opencode_no_branch` — neither present → OpenCode branch not taken (no writes).

### Provisioning (FR-09, AC-05a)
- `test_installer_provisions_plugin_into_dot_opencode_plugins` — shim dropped into
  `.opencode/plugins/`.
- `test_installer_adds_package_dep` — `@opencode-ai/plugin` (and the shim) dep added to
  `.opencode/package.json`.
- `test_installer_appends_plugin_array_entry` — `plugin:[]` in `opencode.json` gains the shim
  entry (if that provisioning shape is used).

### Byte-for-byte preservation — the C10 regression sentinel (FR-10, AC-05b, R-08)
- `test_installer_preserves_mcp_unimatrix_byte_for_byte` — the `mcp.unimatrix` block (type, command,
  env) is unchanged byte-for-byte after provisioning.
- `test_installer_preserves_local_stdio_command` — the local STDIO command entry unchanged.
- `test_installer_preserves_ollama_provider_block` — the local Ollama `provider` block unchanged
  byte-for-byte.
- `test_installer_preserves_non_unimatrix_keys` — all non-Unimatrix keys in `opencode.json`
  preserved (deep-equal on the pre-existing subtree; assert the merge is strictly additive).
  Compare serialized bytes of preserved subtrees, not just parsed equality, to catch key-order/
  formatting drift.

### Retrieval still returns (NFR-03, AC-05c, R-08) — crosses to infra-001
- `test_installer_retrieval_still_returns_after_provision` — after provisioning on the fixture repo,
  MCP `context_*` retrieval still returns results (the C10 proof). Executed in Stage 3c via the
  infra-001 `tools`/`protocol` suites against the provisioned config, OR a node-side spawn of the
  MCP client — whichever the delivery wiring supports. This is the AC-05c binding assertion.

### Idempotence (NFR-06, AC-05d, R-09)
- `test_installer_rerun_no_duplicate_plugin_entry` — running the installer twice adds no duplicate
  `plugin:[]` entry.
- `test_installer_rerun_no_duplicate_package_dep` — no duplicate dep in `.opencode/package.json`.
- `test_installer_rerun_no_key_drift` — preserved keys (mcp.unimatrix, provider, permissions) show
  zero drift across the second run (byte-for-byte identical to the first-run result).

### Untrusted installer input (R-15, security)
- `test_installer_follows_no_injected_path` — a crafted `opencode.json` (e.g. plugin path with
  traversal, symlink target) does not cause the installer to write outside the intended
  `.opencode/` surface or follow an injected path.
- `test_installer_does_not_execute_untrusted_content` — `opencode.json`/`.opencode/` content is
  treated as data, never executed during merge.
- `test_installer_partial_preexisting_plugin_array` — an `opencode.json` with a pre-existing partial
  `plugin:[]` array merges non-clobbering (no overwrite of existing entries; R-09 edge case).

## Edge cases (from Risk Strategy)
- Installer run twice (R-09).
- `opencode.json` with pre-existing partial plugin array (R-09).
- Missing `.opencode/package.json` (create vs merge).
- `opencode.json` present but `.opencode/` absent, and vice versa.

## Notes
- The nan-004 (#1201) prefix-match non-clobber merge is the *principle*; the assertion is on the
  preserved bytes + additive delta, not on internal merge mechanics.
- AC-05 verification is a real installer run on a fixture repo — NOT a mock of the file-write layer
  (drive the real entry point per the behavioral-outcome lens; the outcome is the preserved config
  + working retrieval + provisioned plugin on disk).
