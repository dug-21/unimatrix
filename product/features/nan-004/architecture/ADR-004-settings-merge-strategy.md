## ADR-004: Prefix-Match Identification for settings.json Hook Merge

### Context

`npx unimatrix init` must merge Unimatrix hook entries into `.claude/settings.json` (AC-04, AC-08). The file may contain:
- No hooks section (fresh project).
- Hooks from other tools alongside Unimatrix hooks.
- Unimatrix hooks from a prior init (update scenario, including pre-rename `unimatrix-server` commands).
- Permissions, tool configurations, and other top-level keys.

SR-08 flags merge complexity as high severity. The merge must:
1. Identify existing Unimatrix hooks (to update, not duplicate).
2. Preserve all non-Unimatrix hooks.
3. Preserve non-hook settings (permissions, etc.).
4. Handle the `unimatrix-server` to `unimatrix` rename (hooks from before the rename should be updated, not duplicated).

### Decision

Identify Unimatrix hook entries by command prefix matching. A hook entry is considered "owned by Unimatrix" if its `command` field matches any of these patterns:

```javascript
const UNIMATRIX_PATTERNS = [
  /^unimatrix\s+hook\s/,
  /^unimatrix-server\s+hook\s/,
  /\/unimatrix\s+hook\s/,
  /\/unimatrix-server\s+hook\s/,
];
```

This covers:
- Bare name: `unimatrix hook SessionStart`
- Old bare name: `unimatrix-server hook SessionStart`
- Absolute path: `/path/to/unimatrix hook SessionStart`
- Old absolute path: `/path/to/unimatrix-server hook SessionStart`

For each of the 7 hook events, the merge algorithm:
1. If the event key does not exist in the file, create it with the Unimatrix hook entry.
2. If the event key exists, scan its array for an entry matching a Unimatrix pattern.
3. If found, replace that entry's `command` with the new absolute-path command.
4. If not found, append the Unimatrix hook entry to the array.

Special case: `UserPromptSubmit` retains the `| tee -a ~/.unimatrix/injections/hooks.log` suffix.

The merge function is isolated in `lib/merge-settings.js` with its own test suite. It returns both the merged content and a list of actions taken (for summary output and dry-run mode).

Edge cases:
- **Empty file or missing file**: Start with `{}`, create the hooks structure.
- **Malformed JSON**: Warn to stderr, refuse to modify. Exit with error and instruct user to fix the file.
- **File with only permissions**: Preserve permissions, add hooks section.
- **Duplicate Unimatrix hooks in same event**: Replace the first match, remove subsequent matches (dedup).

### Consequences

**Easier:**
- Handles the `unimatrix-server` to `unimatrix` rename transparently.
- Handles both bare-name and absolute-path commands.
- Isolated module with clear test surface.
- Idempotent: running init twice produces the same result.

**Harder:**
- If a user has a non-Unimatrix tool whose command happens to start with `unimatrix`, it would be misidentified. This is vanishingly unlikely given the specific `unimatrix hook` pattern.
- The regex patterns must be maintained if the binary name changes again. Acceptable since binary names rarely change.
