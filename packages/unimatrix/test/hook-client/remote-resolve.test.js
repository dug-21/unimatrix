"use strict";

// remote-resolve.test.js — vnc-050 C9 unit seams (R-07, R-08, R-18, R-09;
// AC-03, AC-06, AC-28, AC-29). Spawned-process re-proofs live in the bridge
// suites.

const { describe, it } = require("node:test");
const assert = require("assert");
const {
  resolveRemote, parseExtraHeaders, ResolveError, RESERVED_HEADERS, VARS,
} = require("../../lib/hook-client/remote-resolve.js");

const TOKEN = "ab".repeat(32);
const FP = "sha256:" + "cd".repeat(32);
const HSENT = "HDRSENTINEL";
const PH = "p".repeat(16);

function bundle(fp) {
  const obj = {
    v: 2, mcp_url: "https://b.example/mcp", observe_url: "https://b.example/obs",
    token: TOKEN, fp: fp === undefined ? FP : fp,
  };
  return "unimatrix-bundle:" + Buffer.from(JSON.stringify(obj)).toString("base64url");
}
const noStore = () => null;
const store = (cred) => () => cred;
const STORED = {
  mcp_url: "https://s.example/mcp", observe_url: "https://s.example/obs",
  token: "ef".repeat(32), fingerprint: FP, timeouts: { connect_ms: 5 },
};

function rejects(fn, code) {
  let err;
  try { fn(); } catch (e) { err = e; }
  assert.ok(err instanceof ResolveError, "expected ResolveError");
  assert.strictEqual(err.code, code);
  assert.ok(!err.message.includes(TOKEN), "token leaked");
  assert.ok(!err.message.includes(HSENT), "header value leaked");
  return err;
}

describe("resolveRemote precedence", () => {
  it("bundle only -> both consumers from bundle with fp", () => {
    const r = resolveRemote({ [VARS.BUNDLE]: bundle() }, { projectHash: PH, readStore: store(STORED) });
    assert.strictEqual(r.source, "bundle");
    assert.deepStrictEqual(r.mcp, { url: "https://b.example/mcp", token: TOKEN, pinnedFp: FP, tier: "bundle" });
    assert.strictEqual(r.observe.url, "https://b.example/obs");
    assert.strictEqual(r.observe.pinnedFp, FP);
    assert.strictEqual(r.observe.timeouts, null);
  });

  it("bundle fp:null -> CA trust", () => {
    const r = resolveRemote({ [VARS.BUNDLE]: bundle(null) }, {});
    assert.strictEqual(r.mcp.pinnedFp, null);
  });

  it("MCP env pair only -> mcp env, observe from store", () => {
    const r = resolveRemote(
      { [VARS.MCP_URL]: "https://e.example/mcp", [VARS.TOKEN]: TOKEN },
      { projectHash: PH, readStore: store(STORED) },
    );
    assert.strictEqual(r.mcp.tier, "env");
    assert.strictEqual(r.mcp.pinnedFp, null);
    assert.strictEqual(r.observe.tier, "store");
    assert.deepStrictEqual(r.observe.timeouts, { connect_ms: 5 });
    assert.strictEqual(r.source, "mixed");
  });

  it("observe env pair only -> MCP env tier inactive, falls to store", () => {
    const r = resolveRemote(
      { [VARS.URL]: "https://e.example/obs", [VARS.TOKEN]: TOKEN },
      { projectHash: PH, readStore: store(STORED) },
    );
    assert.strictEqual(r.observe.tier, "env");
    assert.strictEqual(r.mcp.url, STORED.mcp_url);
  });

  it("store only", () => {
    const r = resolveRemote({}, { projectHash: PH, readStore: store(STORED) });
    assert.strictEqual(r.source, "store");
    assert.strictEqual(r.mcp.pinnedFp, FP);
  });

  it("stored fingerprint:null -> pinnedFp null", () => {
    const r = resolveRemote({}, { projectHash: PH, readStore: store({ ...STORED, fingerprint: null }) });
    assert.strictEqual(r.mcp.pinnedFp, null);
    assert.strictEqual(r.observe.pinnedFp, null);
  });

  it("none", () => {
    const r = resolveRemote({}, { projectHash: PH, readStore: noStore });
    assert.deepStrictEqual([r.mcp, r.observe, r.source], [null, null, "none"]);
    const r2 = resolveRemote({}, {});
    assert.strictEqual(r2.source, "none");
  });

  it("store read throws -> store_malformed", () => {
    rejects(() => resolveRemote({}, { projectHash: PH, readStore: () => { throw new Error("bad json"); } }), "store_malformed");
  });

  for (const name of [VARS.MCP_URL, VARS.URL, VARS.TOKEN, VARS.FP]) {
    it(`bundle + ${name} -> conflict naming var`, () => {
      const val = name === VARS.TOKEN ? TOKEN : name === VARS.FP ? FP : "https://x.example";
      const e = rejects(() => resolveRemote({ [VARS.BUNDLE]: bundle(), [name]: val }, {}), "conflict");
      assert.ok(e.message.includes(name));
      assert.strictEqual(e.varName, name);
    });
    it(`bundle + ${name}="" -> no conflict (AC-28)`, () => {
      const r = resolveRemote({ [VARS.BUNDLE]: bundle(), [name]: "" }, {});
      assert.strictEqual(r.source, "bundle");
    });
  }

  it("bundle + HEADERS accepted", () => {
    const r = resolveRemote({ [VARS.BUNDLE]: bundle(), [VARS.HEADERS]: JSON.stringify({ "X-A": HSENT }) }, {});
    assert.deepStrictEqual(r.headers, [["X-A", HSENT]]);
  });

  it("bad bundle -> bad_bundle", () => {
    rejects(() => resolveRemote({ [VARS.BUNDLE]: "garbage" }, {}), "bad_bundle");
  });

  for (const urlVar of [VARS.URL, VARS.MCP_URL]) {
    it(`${urlVar} set + TOKEN "" -> exact partial-pair message (AC-29)`, () => {
      const e = rejects(() => resolveRemote({ [urlVar]: "https://x.example", [VARS.TOKEN]: "" }, {}), "partial_pair");
      assert.strictEqual(e.message, `UNIMATRIX_REMOTE_TOKEN is empty; ${urlVar} is set`);
    });
    it(`${urlVar} set + TOKEN unset -> "is not set"`, () => {
      const e = rejects(() => resolveRemote({ [urlVar]: "https://x.example" }, {}), "partial_pair");
      assert.strictEqual(e.message, `UNIMATRIX_REMOTE_TOKEN is not set; ${urlVar} is set`);
    });
  }

  it("TOKEN set + URL \"\" -> partial pair naming both", () => {
    const e = rejects(() => resolveRemote({ [VARS.URL]: "", [VARS.TOKEN]: TOKEN }, {}), "partial_pair");
    assert.strictEqual(e.message, "UNIMATRIX_REMOTE_URL is empty; UNIMATRIX_REMOTE_TOKEN is set");
  });

  it("both halves \"\" -> store used", () => {
    const r = resolveRemote({ [VARS.URL]: "", [VARS.MCP_URL]: "", [VARS.TOKEN]: "" }, { projectHash: PH, readStore: store(STORED) });
    assert.strictEqual(r.source, "store");
  });

  it("whitespace token is non-empty (no trim)", () => {
    const r = resolveRemote({ [VARS.URL]: "https://x.example", [VARS.TOKEN]: " " }, {});
    assert.strictEqual(r.observe.token, " ");
  });

  it("fingerprint without URL -> fp_without_url", () => {
    rejects(() => resolveRemote({ [VARS.FP]: FP }, {}), "fp_without_url");
  });

  it("bad fingerprint format", () => {
    rejects(() => resolveRemote({ [VARS.FP]: "sha256:XYZ", [VARS.URL]: "https://x" , [VARS.TOKEN]: TOKEN }, {}), "bad_fingerprint");
  });

  it("FP + env pairs -> pinnedFp on both consumers", () => {
    const r = resolveRemote({
      [VARS.URL]: "https://x/obs", [VARS.MCP_URL]: "https://x/mcp", [VARS.TOKEN]: TOKEN, [VARS.FP]: FP,
    }, {});
    assert.strictEqual(r.mcp.pinnedFp, FP);
    assert.strictEqual(r.observe.pinnedFp, FP);
    assert.strictEqual(r.source, "env");
  });

  it("bad headers propagate from resolveRemote", () => {
    rejects(() => resolveRemote({ [VARS.HEADERS]: "[" }, {}), "bad_headers");
  });
});

describe("parseExtraHeaders", () => {
  const H = (o) => JSON.stringify(o);
  const reject = (raw, nameInMsg) => {
    const e = rejects(() => parseExtraHeaders(raw), "bad_headers");
    if (nameInMsg) assert.ok(e.message.includes(nameInMsg), e.message);
    assert.strictEqual(e.varName, VARS.HEADERS);
  };

  it("accepts undefined, \"\", {}", () => {
    assert.deepStrictEqual(parseExtraHeaders(undefined), []);
    assert.deepStrictEqual(parseExtraHeaders(""), []);
    assert.deepStrictEqual(parseExtraHeaders("{}"), []);
  });
  it("invalid JSON", () => reject("{" + HSENT));
  it("array", () => reject(H([HSENT])));
  it("number value", () => reject(H({ "X-N": 5 }), "X-N"));
  for (const [label, ch] of [["CR", "\r"], ["LF", "\n"], ["NUL", "\0"]]) {
    it(`${label} in value`, () => reject(H({ "X-C": HSENT + ch }), "X-C"));
  }
  it("8193-byte value rejected; 8192 accepted", () => {
    reject(H({ "X-L": HSENT + "a".repeat(8193 - HSENT.length) }), "X-L");
    assert.strictEqual(parseExtraHeaders(H({ "X-L": "a".repeat(8192) }))[0][1].length, 8192);
  });
  it("33 entries rejected; 32 accepted", () => {
    const o = {};
    for (let i = 0; i < 32; i++) o["X-" + i] = "v";
    assert.strictEqual(parseExtraHeaders(H(o)).length, 32);
    o["X-32"] = HSENT;
    reject(H(o));
  });
  it("bad name token", () => {
    const e = rejects(() => parseExtraHeaders(H({ "Bad Name\r\n": HSENT })), "bad_headers");
    assert.ok(!/[\r\n]/.test(e.message));
  });
  for (const r of RESERVED_HEADERS) {
    const mixed = r.split("").map((c, i) => (i % 2 ? c.toUpperCase() : c)).join("");
    it(`reserved ${mixed}`, () => reject(H({ [mixed]: HSENT }), mixed));
  }
  it("case-duplicate names", () => reject(H({ "X-A": "1", "x-a": HSENT }), "x-a"));
  it("tab inside value accepted", () => {
    assert.deepStrictEqual(parseExtraHeaders(H({ "X-T": "a\tb" })), [["X-T", "a\tb"]]);
  });
  for (const [label, ch] of [["e-acute", "é"], ["euro", "€"], ["emoji", "\u{1F600}"], ["DEL", "\x7f"], ["SOH", "\x01"]]) {
    it(`non-printable-ASCII ${label} rejected`, () => reject(H({ "X-U": HSENT + ch }), "X-U"));
  }
});
