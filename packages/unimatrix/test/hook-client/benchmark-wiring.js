"use strict";
// nan-024 latency gate (AC-06 / ADR-005). Not a *.test.js: run by the `latency-gate` CI job.
//   node test/hook-client/benchmark-wiring.js [--out <json>] [--iters 50] [--warmups 5]
// Budget (D-094, supersedes the D-025 relative formula): absolute per-OS per-path p95 ms, see BUDGETS.
// The `node -e 0` baseline is still measured and reported for information only.
// Paths: cloud project (FNF + sync event class), NOT-A-PROJECT hook, mcp-stdio not-a-project
// (start-to-exit; gated) and mcp-stdio in-project (start-to-initialize; reported only).
// The literal command read from the written config runs through the harness shell, cold, one process per sample.
const fs = require("fs");
const path = require("path");
const https = require("https");
const H = require("../helpers/e2e-harness.js");

const argv = process.argv.slice(2);
const arg = (n, d) => (argv.includes(n) ? argv[argv.indexOf(n) + 1] : d);
const ITERS = Number(arg("--iters", 50));
const WARM = Number(arg("--warmups", 5));
const OUT = arg("--out", null);
// D-096: report-only (CI) never fails on breach; strict (default, local) exits 1 on breach.
const REPORT_ONLY = argv.includes("--report-only") || process.env.LATENCY_REPORT_ONLY === "1";

// D-094 budgets (ms p95). Unlisted mcp-stdio not-a-project uses that OS's not-a-project figure (noted, not measured).
const BUDGETS = {
  linux: { cloud: 74, notProject: 50 },
  darwin: { cloud: 126, notProject: 85 },
  win32: { cloud: 164, notProject: 81 },
};
const BUDGET = BUDGETS[process.platform] || BUDGETS.linux;

function p95(xs) {
  const s = xs.slice().sort((a, b) => a - b);
  return s[Math.max(0, Math.ceil(0.95 * s.length) - 1)];
}
const round = (x) => Math.round(x * 10) / 10;

async function sample(fn) {
  const t0 = process.hrtime.bigint();
  const r = await fn();
  return { ms: Number(process.hrtime.bigint() - t0) / 1e6, r };
}

async function measure(label, fn, check) {
  for (let i = 0; i < WARM; i++) await fn();
  const ms = [];
  for (let i = 0; i < ITERS; i++) {
    const s = await sample(fn);
    if (check) check(s.r);
    ms.push(s.ms);
  }
  const sorted = ms.slice().sort((a, b) => a - b);
  return { label, n: ms.length, min: round(sorted[0]), median: round(sorted[Math.floor(sorted.length / 2)]), p95: round(p95(ms)), max: round(sorted[sorted.length - 1]) };
}

// Cloud RTT: fresh TLS connection + one POST to the stub, in-process (no Node startup), p95 over ITERS.
function rttOnce(stub, cert) {
  return new Promise((resolve, reject) => {
    const t0 = process.hrtime.bigint();
    const req = https.request({ host: "127.0.0.1", port: stub.port, method: "POST", path: "/v1/s/observe", ca: cert.certPem, servername: "localhost", agent: false, headers: { "Content-Type": "application/json" } }, (res) => {
      res.resume();
      res.on("end", () => resolve(Number(process.hrtime.bigint() - t0) / 1e6));
    });
    req.on("error", reject);
    req.end('{"type":"Ack"}');
  });
}

(async () => {
  const cert = H.loadCert();
  if (!cert) { console.error("benchmark-wiring: no TLS cert (set E2E_CERT_DIR)"); process.exit(1); }
  const stub = await H.startStub(cert);
  const f = H.makeFixture();
  const env = H.baseEnv(f, cert, { CODEX_HOME: f.codex });
  const init = await H.runInit(f, env, ["--bundle", H.bundleFor(stub, cert)]);
  if (init.code !== 0) { console.error("benchmark-wiring: init failed: " + init.out + init.err); process.exit(1); }

  const sh = H.shells()[0];
  const line = (ev) => H.hookCommand(path.join(f.proj, ".claude", "settings.local.json"), ev);
  const hook = (ev, cwd) => {
    const l = line(ev);
    const input = JSON.stringify({ session_id: "bench", hook_event_name: ev, cwd, prompt: "p", tool_name: "Read", tool_input: {}, tool_response: "ok" });
    return () => H.runLine(sh, l, cwd, env, input);
  };
  const j = JSON.parse(fs.readFileSync(path.join(f.home, ".claude.json"), "utf8"));
  const entry = j.projects[Object.keys(j.projects)[0]].mcpServers.unimatrix;
  const mcp = (cwd) => () => H.run(entry.command, entry.args || [], { spawn: { cwd, env: Object.assign({}, env, entry.env || {}) }, input: "" });

  const base = await measure("node -e 0 (baseline)", () => H.run(process.execPath, ["-e", "0"], { spawn: { env } }));
  const shellBase = await measure("node -e 0 via " + sh.name + " (info)", () => H.runLine(sh, '"' + process.execPath + '" -e 0', f.other, env, ""));
  const rtts = [];
  for (let i = 0; i < WARM; i++) await rttOnce(stub, cert);
  for (let i = 0; i < ITERS; i++) rtts.push(await rttOnce(stub, cert));
  const rtt = round(p95(rtts));
  const rows = [];
  const gated = [
    ["cloud project, FNF event (PostToolUse)", hook("PostToolUse", f.proj), (r) => { if (r.code !== 0) throw new Error("hook exit " + r.code + ": " + r.err); }],
    ["cloud project, sync event (UserPromptSubmit)", hook("UserPromptSubmit", f.proj), (r) => { if (r.code !== 0) throw new Error("hook exit " + r.code + ": " + r.err); }],
    ["NOT-A-PROJECT hook (PostToolUse)", hook("PostToolUse", f.other), (r) => { if (r.code !== 0 || r.err !== "") throw new Error("not-a-project not silent: " + r.err); }],
    ["mcp-stdio NOT-A-PROJECT start-to-exit", mcp(f.other), (r) => { if (r.code !== 0) throw new Error("mcp exit " + r.code + ": " + r.err); }],
  ];
  for (const [label, fn, check] of gated) {
    const m = await measure(label, fn, check);
    // D-094: absolute per-OS budget; cloud paths vs not-a-project paths (mcp-stdio not-a-project uses the not-a-project figure).
    m.limit = /^cloud project/.test(label) ? BUDGET.cloud : BUDGET.notProject;
    m.pass = m.p95 <= m.limit;
    rows.push(m);
  }
  // reported only: mcp-stdio in-project, start to initialize response
  const init1 = await measure("mcp-stdio in-project start-to-initialize (info)", () => H.mcpInitialize(entry, f.proj, env));

  const report = { platform: process.platform, node: process.version, shell: sh.name, baseline: base, shellBaseline: shellBase, budgetsMs: BUDGET, rttP95Ms: rtt, rows, info: [shellBase, init1] };
  const lines = ["| case | n | median ms | p95 ms | limit ms | result |", "|---|---|---|---|---|---|"];
  lines.push("| " + base.label + " | " + base.n + " | " + base.median + " | " + base.p95 + " | - | baseline |");
  for (const m of rows) lines.push("| " + m.label + " | " + m.n + " | " + m.median + " | " + m.p95 + " | " + m.limit + " | " + (m.pass ? "PASS" : "BREACH") + " |");
  for (const m of report.info) lines.push("| " + m.label + " | " + m.n + " | " + m.median + " | " + m.p95 + " | - | info |");
  const md = "### Latency gate (" + process.platform + ", node " + process.version + ", shell " + sh.name + ")\n\n" + lines.join("\n") + "\n";
  console.log(md);
  if (process.env.GITHUB_STEP_SUMMARY) { try { fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, md + "\n"); } catch (_e) { /* best effort */ } }
  if (OUT) fs.writeFileSync(OUT, JSON.stringify(report, null, 2));
  await stub.close();
  f.cleanup();
  const breaches = rows.filter((m) => !m.pass);
  if (REPORT_ONLY) console.log("REPORT-ONLY (D-096): breaches are informational" + (breaches.length ? ": " + breaches.map((m) => m.label + " p95 " + m.p95 + " > " + m.limit).join("; ") : " (none)"));
  else if (breaches.length) { console.error("benchmark-wiring: BREACH: " + breaches.map((m) => m.label + " p95 " + m.p95 + " > " + m.limit).join("; ")); process.exit(1); }
  process.exit(0);
})().catch((e) => { console.error("benchmark-wiring: " + (e && e.stack || e)); process.exit(REPORT_ONLY ? 0 : 1); });
