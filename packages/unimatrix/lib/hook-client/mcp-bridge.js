"use strict";

// mcp-bridge.js (C2 entry, Scope A — ADR-001/002/004). Pure-Node-stdlib
// stdio<->Streamable-HTTP MCP bridge. argv: `node <bridge> [projectHash]`.
// Resolves the credential via remote-resolve.js (bundle > env > store; token
// NEVER on the command line / logged — NFR-06), holds a pinned or CA-verified
// HTTPS connection to mcp_url, proxies stdio JSON-RPC. FAIL-LOUD (persistent, not fail-open).

const { resolveRemote, ResolveError } = require("./remote-resolve.js");
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

  const env = (deps && deps.env) || process.env;
  const fail = (m) => { errOut("mcp-bridge: " + m + "\n"); exit(1); return null; };
  let r;
  try {
    r = resolveRemote(env, { projectHash });
  } catch (e) {
    if (e instanceof ResolveError) return fail(e.message);
    throw e;
  }
  const m = r.mcp;
  if (m === null) return fail("no MCP credential (set UNIMATRIX_BUNDLE, UNIMATRIX_REMOTE_MCP_URL + UNIMATRIX_REMOTE_TOKEN, or run init --bundle)");
  if (!/^https:/.test(m.url) || !isNonEmptyString(m.token)) return fail("credential missing https mcp_url or token");
  // Null fingerprint = CA trust (ADR-006 supersedes vnc-039 ADR-005 refusal).
  return HttpSession.create({ mcpUrl: m.url, token: m.token, pinnedFp: m.pinnedFp, extraHeaders: r.headers, exit, errOut });
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
  const env = (deps && deps.env) || process.env;
  const projectHash = argv[2];
  // Headless (FR-21): bundle or env MCP URL alone needs no projectHash.
  if (!isNonEmptyString(projectHash) && !isNonEmptyString(env.UNIMATRIX_BUNDLE) && !isNonEmptyString(env.UNIMATRIX_REMOTE_MCP_URL)) {
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
