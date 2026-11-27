# 1036-agent-17-removal — C16 removal + conformance greps

Unimatrix entries consulted: #5962, #5955

## Files
- Deleted: packages/unimatrix/lib/atomic-write.js (no remaining requirers)
- Modified: lib/opencode-install.js (dead `isWithinProject` removed; superseded by safe-write `containWithin`)
- Modified: test/opencode-install.test.js (dropped traversal-guard unit test; covered by safe-write `outside-project` tests)
- Modified: test/claude-json.test.js (retired-name assertions moved to contract-conformance; mtime/statSync source check kept)
- New: test/contract-conformance.test.js (S-5 names, atomic-write absent/unrequired, status no private codexHome, no .pid temp names, FR-20 raw fs calls only in trust.js/safe-write.js with install-store allowlist [dir-aside renameSync, .complete marker], writer cores sync-only)

Already removed by earlier waves: atomicWriteText, removeOwnedClaudeLegacy, gitIgnored, GITIGNORE_WARNING, status.codexHome, install-store private writers.

## Tests
contract-conformance 6/6, opencode-install 58/58, claude-json 10/10, safe-write 22/22, install-store 21/21, status 12/12, wire 91/92 (failing: test_ac3_parent_dir_link_outside_root_refused — concurrent agent's new AC-3 test, unrelated to removal). check-zero-deps OK, check-test-count PASS (1410).

## Footprint
test_remote_install_under_440kb still FAILS: 472,152 (lib 403,732, skills 68,420); was 473,881; main 431,255; limit 440,000 (unchanged). Growth vs main led by config-dirs +6.3K, trust +6.2K, wire-plan +5.9K, status +5.4K, git-exclude +4.7K, safe-write +4.4K, claude-json +3.7K, opencode-install +3.0K. No further behavior-neutral dead code found; needs ~32 KB — escalate.

## Knowledge Stewardship
- Queried: context_briefing -- ADR-003/005 (one writer, core location), size-gate lessons
- Stored: nothing novel to store -- grep-conformance pattern is feature-local
