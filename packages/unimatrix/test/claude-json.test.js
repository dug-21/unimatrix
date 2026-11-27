"use strict";
const { describe, it } = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const cj = require("../lib/claude-json");
const { buildMcpEntry } = require("../lib/mcp-entry");

const mk = () => fs.mkdtempSync(path.join(os.tmpdir(), "um-cj7-"));
const entry = buildMcpEntry("linux", "/h", "/n/node");
const rd = (f) => JSON.parse(fs.readFileSync(f, "utf8"));
const tmps = (d) => fs.readdirSync(d).filter((n) => n.includes(".tmp"));
function setup(content) {
  const dir = mk(), root = mk();
  const file = path.join(dir, ".claude.json");
  if (content !== undefined) fs.writeFileSync(file, typeof content === "string" ? content : JSON.stringify(content, null, 2));
  return { dir, root, file, key: fs.realpathSync(root) };
}

describe("C7 claude-json pure mutates", function () {
  it("destination + G2 removal in one write; two legs; fixpoint; dry-run parity", function () {
    const s = setup({ mcpServers: { unimatrix: entry, other: { command: "o" } }, theme: "x" });
    const before = fs.readFileSync(s.file);
    const o = { file: s.file, projectRoot: s.root, entry };
    const dry = cj.claudeMcpLegs({ ...o, dryRun: true });
    assert.ok(before.equals(fs.readFileSync(s.file)));
    const legs = cj.claudeMcpLegs(o);
    assert.deepStrictEqual(legs.map((l) => l.action), ["created", "removed"]);
    assert.deepStrictEqual(dry.map((l) => l.action), ["created", "removed"]);
    const j = rd(s.file);
    assert.deepStrictEqual(j.mcpServers, { other: { command: "o" } });
    assert.deepStrictEqual(j.projects[s.key].mcpServers.unimatrix, entry);
    assert.strictEqual(cj.computeClaudeMcp(fs.readFileSync(s.file, "utf8"), o).text, null);
    assert.deepStrictEqual(cj.claudeMcpLegs(o).map((l) => l.action), ["unchanged"]);
    assert.deepStrictEqual(tmps(s.dir), []);
  });
  it("R1-only change keeps mcp leg unchanged; empty mcpServers kept", function () {
    const s = setup();
    fs.writeFileSync(s.file, JSON.stringify({ mcpServers: { unimatrix: entry } }));
    const s2key = fs.realpathSync(s.root);
    const j0 = rd(s.file); j0.projects = { [s2key]: { mcpServers: { unimatrix: entry } } };
    fs.writeFileSync(s.file, JSON.stringify(j0));
    const legs = cj.claudeMcpLegs({ file: s.file, projectRoot: s.root, entry });
    assert.deepStrictEqual(legs.map((l) => l.action), ["unchanged", "removed"]);
    assert.deepStrictEqual(rd(s.file).mcpServers, {});
  });
  it("foreign top-level unimatrix kept with warning (D-065)", function () {
    const s = setup({ mcpServers: { unimatrix: { command: "mine" } } });
    const legs = cj.claudeMcpLegs({ file: s.file, projectRoot: s.root, entry });
    assert.deepStrictEqual(legs.map((l) => l.action), ["created"]);
    assert.ok(/mcpServers\.unimatrix holds a non-Unimatrix/.test(legs[0].warning));
    assert.deepStrictEqual(rd(s.file).mcpServers.unimatrix, { command: "mine" });
  });
  it("foreign local entry: skipped-foreign, G2 not applied, bytes unchanged", function () {
    const s = setup();
    fs.writeFileSync(s.file, JSON.stringify({ mcpServers: { unimatrix: entry }, projects: { [s.key]: { mcpServers: { unimatrix: { command: "mine" } } } } }));
    const before = fs.readFileSync(s.file);
    const legs = cj.claudeMcpLegs({ file: s.file, projectRoot: s.root, entry });
    assert.strictEqual(legs.length, 1);
    assert.strictEqual(legs[0].action, "skipped-foreign");
    assert.ok(legs[0].warning);
    assert.ok(before.equals(fs.readFileSync(s.file)));
  });
  it("unparseable / non-object: ClaudeJsonError, bytes unchanged, no temp", function () {
    for (const bad of ["{oops", "[1]"]) {
      const s = setup(bad);
      assert.throws(() => cj.claudeMcpLegs({ file: s.file, projectRoot: s.root, entry }), cj.ClaudeJsonError);
      assert.strictEqual(fs.readFileSync(s.file, "utf8"), bad);
      assert.deepStrictEqual(tmps(s.dir), []);
    }
  });
  it("fidelity: indent, key order, __proto__ own key, big integer text", function () {
    for (const ind of ["\t", "    ", "  "]) {
      const raw = '{\n' + ind + '"z": 9007199254740993,\n' + ind + '"__proto__": {"a": 1.0},\n' + ind + '"a": "x 12 \\" 9007199254740993"\n}';
      const s = setup(raw);
      cj.claudeMcpLegs({ file: s.file, projectRoot: s.root, entry });
      const out = fs.readFileSync(s.file, "utf8");
      assert.ok(out.includes("\n" + ind + '"z": 9007199254740993,'), out);
      assert.ok(out.includes('"a": 1.0'));
      assert.ok(out.includes('"x 12 \\" 9007199254740993"'));
      assert.deepStrictEqual(Object.keys(JSON.parse(out)), ["z", "__proto__", "a", "projects"]);
      assert.ok(Object.prototype.hasOwnProperty.call(JSON.parse(out), "__proto__"));
    }
  });
  it("new file created 0600; only o.file touched (D-028)", function () {
    const s = setup();
    const home = mk();
    const dflt = path.join(home, ".claude.json");
    fs.writeFileSync(dflt, JSON.stringify({ projects: { [s.key]: { mcpServers: { unimatrix: entry } } } }));
    const old = new Date(2020, 1, 1);
    fs.utimesSync(dflt, old, old);
    const b = fs.readFileSync(dflt);
    const r = cj.writeClaudeMcp({ file: s.file, projectRoot: s.root, entry });
    assert.strictEqual(r.action, "created");
    if (process.platform !== "win32") assert.strictEqual(fs.statSync(s.file).mode & 0o777, 0o600);
    assert.ok(b.equals(fs.readFileSync(dflt)));
    assert.strictEqual(fs.statSync(dflt).mtimeMs, old.getTime());
  });
  it(".mcp.json removal legs; absent file not created; foreign warning without leg", function () {
    const root = mk();
    const file = path.join(root, ".mcp.json");
    assert.deepStrictEqual(Array.from(cj.removeOwnedMcpJsonLeg(root, false)), []);
    assert.ok(!fs.existsSync(file));
    fs.writeFileSync(file, JSON.stringify({ mcpServers: { unimatrix: entry, o: { command: "o" } } }, null, "\t"));
    const legs = cj.removeOwnedMcpJsonLeg(root, false);
    assert.strictEqual(legs[0].action, "removed");
    assert.deepStrictEqual(rd(file), { mcpServers: { o: { command: "o" } } });
    assert.ok(fs.readFileSync(file, "utf8").includes('\n\t"mcpServers"'));
    fs.writeFileSync(file, '{"mcpServers":{"unimatrix":{"command":"mine"}}}');
    const l2 = cj.removeOwnedMcpJsonLeg(root, false);
    assert.strictEqual(l2.length, 0);
    assert.strictEqual(l2.warnings.length, 1);
    fs.writeFileSync(file, "{bad");
    assert.throws(() => cj.removeOwnedMcpJson(root, false), cj.ClaudeJsonError);
  });
  it("readClaudeMcpState: wired / missing / foreign / unreadable; win32 key", function () {
    const s = setup();
    assert.strictEqual(cj.readClaudeMcpState(s.file, s.root).status, "missing");
    fs.writeFileSync(s.file, JSON.stringify({ projects: {} }));
    assert.strictEqual(cj.readClaudeMcpState(s.file, s.root).status, "missing");
    fs.writeFileSync(s.file, JSON.stringify({ projects: { [s.key]: { mcpServers: { unimatrix: entry } } } }));
    const w = cj.readClaudeMcpState(s.file, s.root);
    assert.deepStrictEqual(w, { status: "wired", file: s.file, key: s.key });
    fs.writeFileSync(s.file, JSON.stringify({ projects: { [s.key]: { mcpServers: { unimatrix: { command: "x" } } } } }));
    assert.strictEqual(cj.readClaudeMcpState(s.file, s.root).status, "foreign");
    fs.writeFileSync(s.file, "{nope");
    assert.strictEqual(cj.readClaudeMcpState(s.file, s.root).status, "unreadable");
    fs.writeFileSync(s.file, JSON.stringify({ projects: { "c:/Users/a": { mcpServers: { unimatrix: entry } } } }));
    const r = cj.readClaudeMcpState(s.file, "C:\\Users\\a", "win32");
    assert.strictEqual(r.status, "wired");
    assert.strictEqual(r.key, "c:/Users/a");
  });
  it("atomicWriteText re-export, mtime CAS and removeOwnedClaudeLegacy gone", function () {
    assert.strictEqual(cj.atomicWriteText, undefined);
    assert.strictEqual(cj.removeOwnedClaudeLegacy, undefined);
    const src = fs.readFileSync(path.join(__dirname, "../lib/claude-json.js"), "utf8");
    assert.ok(!/mtimeMs|statSync|atomic-write|o\.home/.test(src));
  });
});
