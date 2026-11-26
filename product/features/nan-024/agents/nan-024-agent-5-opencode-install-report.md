# nan-024-agent-5-opencode-install report

Unimatrix entries consulted: #5894 (ADR-006), #5773, #5743, #5763, #5837

## Report
- Modified: packages/unimatrix/lib/opencode-install.js (608 -> ~275 lines), packages/unimatrix/test/opencode-install.test.js
- Deleted: all plugin provisioning, resolveOpencodeTransport, binary/bridge variants, maybeProvisionOpenCode.
- Exports: detectsOpenCode, writeOpencodeMcp({home,platform,node,harnessSelected,dryRun}), removeOwnedOpencodeProjectMcp(root,dryRun), buildOpencodeEntry, opencodeLeg, isWithinProject, detectIndent, readJsonSafe. (isPlainObject/deepEqual kept internal, not exported, as before.)
- Entry = {type:"local", command:<argv>} (no `enabled`, per task/test plan; pseudocode listed enabled:true).
- Foreign leg carries `warning` field (D-065 text); action "skipped-foreign".
- Tests: opencode-install.test.js 40 -> 45, all pass.
- Not done (out of scope): wire.js/init.js still import maybeProvisionOpenCode and call old writeOpencodeMcp signature (Wave 4); c14-opencode.test.js depends on wire and will break until Wave 4.

## Knowledge Stewardship
- Queried: context_briefing -- ADR-006 and prior opencode mcp pattern #5773 (superseded by argv form)
- Stored: nothing novel to store -- straightforward deletion/port
