# C11 — Ownership (`lib/hook-ownership.js`)

## Purpose
Add the pure predicate `isLegacyOpencodePlugin(el)` (FR-15, §6a canonical name).

```
const LEGACY_OC_PLUGIN = "@dug-21/unimatrix-opencode-plugin"

function isLegacyOpencodePlugin(el) -> boolean
  if typeof el !== "string": return false
  if el === LEGACY_OC_PLUGIN: return true
  if !el.startsWith(LEGACY_OC_PLUGIN + "@"): return false
  spec = el.slice(LEGACY_OC_PLUGIN.length + 1)
  return spec.length > 0 and !spec.includes("/")
```

Export added to `module.exports`. No other change.

## Key tests
Positive: exact name, `@1.2.3`, `@latest`, `@^0.4`.
Negative: `@dug-21/unimatrix-opencode-plugin-extra`, `@other/unimatrix-opencode-plugin`, `unimatrix-opencode-plugin`, `file:./x/unimatrix-opencode-plugin`, `@dug-21/unimatrix-opencode-plugin@`, `...@a/b`, `null`, `1`, `{}`, `["@dug-21/unimatrix-opencode-plugin"]`.
