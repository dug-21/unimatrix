# vnc-050-agent-8-remote-resolve — C9 Remote Resolver

## Report
- Created: packages/unimatrix/lib/hook-client/remote-resolve.js, packages/unimatrix/test/hook-client/remote-resolve.test.js
- Exports: resolveRemote, parseExtraHeaders, ResolveError, RESERVED_HEADERS, VARS (per pseudocode). Tuples carry `tier`; observe carries `timeouts` (store tier: cred.timeouts; else null).
- parseExtraHeaders returns Array<[name,value]> (Object.entries order).
- Tests: remote-resolve 54/54 pass; full hook-client set 903/903 pass.
- Size gate: PASS, stripped 109775/110000 (225 B headroom), raw 196800/200000. Code compacted to fit; other Wave 1-3 additions will exceed the gate — human cap decision needed (not self-raised).
- package.json/lockfile unchanged; no deps. Not committed per instruction.
- Incident: briefly ran `git stash` on lib/hook-client, which captured the concurrent agent's bundle.js edit; popped immediately and cleanly (bundle.js modification intact, bundle tests pass).

## Knowledge Stewardship
- Queried: context_search pattern (#5123 credstore/config test-helper trap) and decision topic vnc-050 (ADR-001/005/006) — consistent with pseudocode.
- Stored: nothing novel to store — implementation follows ADR-006/007 directly.
