# nan-024-agent-10-3b-rework Report

Unimatrix entries consulted: #5905, #5894, #251, #5830

## Report
- entry.test.js: removed import of deleted PLUGIN_ENTRY_FILE/PLUGIN_PACKAGE/PLUGIN_EXPORT from lib/opencode-install.js (ADR-006); constants now defined in the test (plugin's own identity); assertions kept (package.json main === "unimatrix.js"; export name checked against UnimatrixObservePlugin.name). emit.js and opencode-install.js untouched.
- NFR-05: new test/helpers/home-guard.js `assertTmpHome` (throws if HOME unset, equals real home, or not under os.tmpdir()). Called in wire-fixture (makeHome, wireOpts), e2e-harness (makeFixture, baseEnv), launcher-fixture (home), shim-fixture (baseEnv). Unit test test/home-guard.test.js.
- Results: opencode-plugin set 37/37; wiring-e2e 29/29; test:package 728 tests, 727 pass, 0 fail, 1 skipped. No lib/ changes (size gate unaffected); no dependency changes.

## Knowledge Stewardship
- Queried: context_briefing -- #5905 wire/init per-machine writes, #251 test home leak lesson applied
- Stored: nothing novel to store -- guard is a straightforward application of #251
