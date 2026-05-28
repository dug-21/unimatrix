# Test Plan: C4 -- README Configuration Section

## Component

Update README.md Configuration section (~line 238) to lead with per-project config as canonical. Update container quickstart (~line 62) to remove `/etc/unimatrix/` reference.

## Risks Covered

- **R-05** (Low): README merge conflict with concurrent PRs

## Unit Test Expectations

No unit tests apply. README.md is documentation only.

## Code Review Checklist

### Configuration Section (~line 238)
- [ ] Per-project `~/.unimatrix/{hash}/config.toml` presented first as canonical, primary location
- [ ] Global `~/.unimatrix/config.toml` presented as optional cross-project defaults layer
- [ ] Replace semantics for list fields preserved (existing explanation)
- [ ] No reference to `/etc/unimatrix/config.toml` as primary container config pattern

### Container Quickstart (~line 62)
- [ ] Reference to "read-only bind mount at `/etc/unimatrix/config.toml`" removed or updated
- [ ] Replacement text references config in the data volume

### Edit Boundaries
- [ ] No changes outside the Configuration section and container description line
- [ ] No changes to feature list, installation instructions, or other sections

## Content Assertions

- [ ] String `per-project` appears before `global` in the Configuration section (AC-04)
- [ ] String `/etc/unimatrix/config.toml` does NOT appear as a recommended config path
- [ ] String `canonical` or `primary` appears in the per-project description
- [ ] String `defaults` appears in the global config description
- [ ] String `~/.unimatrix/{hash}/config.toml` or equivalent path notation present

## Pre-Delivery Check

- [ ] No open PRs touching README.md Configuration section (R-05)
- [ ] `git diff README.md` shows changes only in the Configuration section and container quickstart line

## Edge Cases

- **Markdown formatting**: Ensure code backticks, links, and headers render correctly after edit.
- **Line count shift**: If the Configuration section grows or shrinks, line numbers in other documentation referencing README.md may drift. Not a functional concern but noted.
