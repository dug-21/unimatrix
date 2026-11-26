"use strict";
const test = require("node:test");
const assert = require("node:assert");
const {
  OWNED_FORMS, KNOWN_EVENTS, isUnimatrixCommand, isUnimatrixMcpEntry, normalizeMcpArgv,
} = require("../lib/hook-ownership");

function tryShim() {
  for (const m of ["../lib/merge-settings", "../lib/wire"]) {
    try {
      const f = require(m).buildShimCommand;
      if (typeof f === "function") return f;
    } catch (_) { /* absent */ }
  }
  return null;
}

test("positive: every KnownEvent on every literal form", () => {
  for (const e of KNOWN_EVENTS) {
    const cmds = [
      `unimatrix hook ${e}`,
      `unimatrix-server hook ${e}`,
      `LD_LIBRARY_PATH=/x/y /opt/bin/unimatrix hook ${e}`,
      `/usr/local/bin/unimatrix hook ${e}`,
      `"C:\\Program Files\\u\\unimatrix.exe" hook ${e}`,
      `npx -y @dug-21/unimatrix@1.2.3 hook ${e}`,
      `npx unimatrix hook ${e}`,
      `unimatrix hook ${e} --provider codex-cli`,
      `unimatrix hook ${e} --provider=codex-cli`,
    ];
    for (const c of cmds) assert.ok(isUnimatrixCommand(c), c);
    assert.equal(isUnimatrixCommand(`echo x/unimatrix hook ${e}`), false);
  }
});

test("positive: node client / npx-cache / shim forms", () => {
  const ok = [
    "node /a/b/hook-client/index.js Stop",
    'node "/a b/hook-client/index.js" SessionStart',
    "node C:\\x\\hook-client\\index.js Stop --provider codex-cli",
    "node /home/u/.npm/_npx/abc/node_modules/@dug-21/unimatrix/lib/hook-client/index.js PreToolUse",
    '$HOME/.unimatrix/bin/unimatrix hook FutureEvent',
    '"$HOME/.unimatrix/bin/unimatrix" hook Stop',
    "~/.unimatrix/bin/unimatrix.cmd hook Stop",
    "'/home/u/.unimatrix/bin/unimatrix' hook Stop",
    '"/home/my user/.unimatrix/bin/unimatrix" hook NewEvent',
    '"C:\\Users\\a b\\.unimatrix\\bin\\unimatrix.cmd" hook Stop --provider codex-cli',
    "'C:/Users/a/.unimatrix/bin/unimatrix.cmd' hook Stop",
  ];
  for (const c of ok) assert.ok(isUnimatrixCommand(c), c);
});

test("negative table", () => {
  const bad = [
    "my-unimatrix-hook.sh Stop", "echo unimatrix hook Stop", "unimatrix-backup hook Stop",
    "node other/index.js Stop", "unimatrix hook NotAnEvent", "unimatrix hook",
    "LD_LIBRARY_PATH=x /bin/echo unimatrix hook Stop", "cat /x/hook-client/index.js",
    "ls /a/hook-client/index.js Stop", "", "   ", null, undefined, 5, {}, [],
  ];
  for (const c of bad) assert.equal(isUnimatrixCommand(c), false, String(c));
});

test("every arm requires hook <event> or hook-client adjacency", () => {
  for (const f of OWNED_FORMS) {
    assert.match(f.re.source, /hook\\s\+|hook-client|hook\\s\+/, f.name);
  }
});

test("property: buildShimCommand round trip", (t) => {
  const build = tryShim();
  if (!build) return t.skip("buildShimCommand not yet exported");
  for (const e of KNOWN_EVENTS) {
    for (const p of [undefined, "codex-cli"]) {
      for (const platform of ["linux", "darwin", "win32"]) {
        for (const home of ["/home/u", "/home/my user", "/home/ünï", "/a/b/c/d/e/f"]) {
          const cmd = build(e, p, platform, home);
          assert.ok(isUnimatrixCommand(cmd), cmd);
        }
      }
    }
  }
});

test("MCP: owned forms", () => {
  const owned = [
    { command: "node", args: ["/x/lib/mcp-bridge.js", "abc"] },
    { command: "/usr/bin/node", args: ["C:\\x\\mcp-bridge.js"] },
    { command: "npx", args: ["-y", "unimatrix", "mcp-bridge"] },
    { command: "npx", args: ["@dug-21/unimatrix@1.0.0", "mcp-bridge"] },
    { command: "unimatrix" },
    { command: "/bin/unimatrix-server", args: ["serve"] },
    { command: "unimatrix.exe", args: ["mcp-stdio"] },
    { command: "sh", args: ["-c", 'exec "$HOME/.unimatrix/bin/unimatrix" mcp-stdio'] },
    { command: "cmd", args: ["/c", '"C:\\Users\\a\\.unimatrix\\bin\\unimatrix.cmd" mcp-stdio'] },
    { command: "node", args: ["/h/.unimatrix/bin/shim-entry.js", "mcp-stdio"] },
    { command: "C:\\n\\node.exe", args: ["C:\\h\\.unimatrix\\bin\\shim-entry.js", "mcp-stdio"] },
    { command: ["node", "/x/mcp-bridge.js", "h"] },
  ];
  for (const e of owned) assert.ok(isUnimatrixMcpEntry("unimatrix", e), JSON.stringify(e));
});

test("MCP: foreign / near-miss", () => {
  const bad = [
    { command: "my-tool" },
    { command: "node", args: ["other.js"] },
    { command: "npx", args: ["other-pkg", "mcp-bridge"] },
    { command: "unimatrix-server", args: ["--flag-unknown"] },
    { command: "node", args: ["/h/.unimatrix/bin/shim-entry.js", "other"] },
    {}, null, "str", [], { command: 5 }, { command: [] },
  ];
  for (const e of bad) assert.equal(isUnimatrixMcpEntry("unimatrix", e), false, JSON.stringify(e));
  assert.equal(isUnimatrixMcpEntry("other", { command: "unimatrix" }), false);
  assert.equal(isUnimatrixMcpEntry(undefined, { command: "unimatrix" }), false);
});

test("normalizeMcpArgv", () => {
  assert.deepEqual(normalizeMcpArgv({ command: "a", args: [1] }), ["a", "1"]);
  assert.deepEqual(normalizeMcpArgv({ command: ["a", "b"] }), ["a", "b"]);
  assert.equal(normalizeMcpArgv(null), null);
  assert.equal(normalizeMcpArgv({ command: 1 }), null);
});

test("KNOWN_EVENTS superset of codex-install CODEX_EVENTS", () => {
  const { CODEX_EVENTS } = require("../lib/codex-install");
  for (const e of CODEX_EVENTS) assert.ok(KNOWN_EVENTS.includes(e), e);
});
