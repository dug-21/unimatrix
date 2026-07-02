"use strict";

// mcp-bridge.js (C2 entry, Scope A — ADR-001/002/004). Pure-Node-stdlib
// stdio<->Streamable-HTTP MCP bridge. argv: `node <bridge> <projectHash>`.
// Reads the credential from the out-of-tree store at spawn (token NEVER on the
// command line / logged — NFR-06), holds a fingerprint-pinned HTTPS connection
// to mcp_url, proxies stdio JSON-RPC. FAIL-LOUD (persistent, not fail-open).

const credstore = require("./credstore.js");
const { StdioFramer } = require("./mcp-bridge/stdio-frame.js");
const { HttpSession } = require("./mcp-bridge/http-session.js");
const { Lifecycle } = require("./mcp-bridge/lifecycle.js");

function isNonEmptyString(v) {
  return typeof v === "string" && v.length > 0;
}

// Resolve + validate the credential the bridge owns. Returns an HttpSession or
// exits non-zero (loud, token-free). `deps` injectable for tests.
function buildSession(projectHash, deps) {
  const errOut = (deps && deps.errOut) || ((s) => process.stderr.write(s));
  const exit = (deps && deps.exit) || ((c) => process.exit(c));

  let cred;
  try {
    cred = credstore.read(projectHash);
  } catch (e) {
    errOut("mcp-bridge: " + (e && e.message) + "\n");
    exit(1);
    return null;
  }
  if (cred === null) {
    errOut("mcp-bridge: no credential for project " + projectHash + " (run init --bundle)\n");
    exit(1);
    return null;
  }
  if (!isNonEmptyString(cred.mcp_url) || !/^https:/.test(cred.mcp_url) || !isNonEmptyString(cred.token)) {
    errOut("mcp-bridge: credential missing mcp_url or token\n");
    exit(1);
    return null;
  }
  if (cred.fingerprint === null || cred.fingerprint === undefined) {
    errOut("mcp-bridge: this credential has no pinned fingerprint (cloud MCP requires a v:2 bundle)\n");
    exit(1);
    return null;
  }

  return HttpSession.create({
    mcpUrl: cred.mcp_url,
    token: cred.token,
    pinnedFp: cred.fingerprint,
    exit,
    errOut,
  });
}

// Wire the framer/lifecycle/session and run until stdin EOF. `deps` injectable.
function run(session, deps) {
  const stdin = (deps && deps.stdin) || process.stdin;
  const stdout = (deps && deps.stdout) || process.stdout;
  const exit = (deps && deps.exit) || ((c) => process.exit(c));

  const framer = new StdioFramer(stdin, stdout);
  // Forward deps so the lifecycle's errOut floor + injectable clock are wired
  // (N3: run() previously built Lifecycle with no deps → stderr-only, untestable).
  const lifecycle = new Lifecycle(session, deps);

  framer.onMessage(async (msg) => {
    let response = null;
    try {
      response = await lifecycle.handle(msg);
    } catch (_err) {
      return; // pin mismatch already exited loud; keep bridge alive otherwise
    }
    if (response !== null) framer.write(response);
  });

  stdin.on("end", async () => {
    try {
      await session.teardown();
    } catch (_err) {
      // best-effort
    }
    exit(0);
  });
}

function main(argv, deps) {
  argv = argv || process.argv;
  const errOut = (deps && deps.errOut) || ((s) => process.stderr.write(s));
  const exit = (deps && deps.exit) || ((c) => process.exit(c));
  const projectHash = argv[2];
  if (!isNonEmptyString(projectHash)) {
    errOut("usage: unimatrix mcp-bridge <projectHash>\n");
    exit(2);
    return;
  }
  const session = buildSession(projectHash, deps);
  if (session === null) return; // buildSession already exited loud
  run(session, deps);
}

if (require.main === module) {
  main();
}

module.exports = { main, buildSession, run, isNonEmptyString };
