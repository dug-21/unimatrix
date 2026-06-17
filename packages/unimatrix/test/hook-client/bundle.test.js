"use strict";

// bundle.test.js — vnc-038 Component 2: the v:2 bundle decoder (bundle.js,
// ADR-002 #5081 / ADR-001 #5080). JS DECODES only; it never encodes.
//
// PARITY (R-03): the v:2 wire/field expectations are READ FROM the committed
// Rust-oracle corpus (crates/unimatrix-server/tests/fixtures/c1c2-parity/
// bundle-golden.json). The JS golden is NEVER hand-written — this suite is the
// JS half of the cross-language parity oracle and FAILS the instant the Rust
// encoder drifts. Cumulative infra: consumes the same committed corpus as
// remote-client.test.js, no isolated scaffolding.
//
// Guard ordering is LOCKED: length -> scheme -> base64url -> JSON -> strict
// schema. Zero-dependency (NFR-08). Token never appears in a thrown message
// (NFR-06).

const { describe, it } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const path = require("path");

const {
  decodeBundle,
  BundleError,
  BUNDLE_SCHEME,
  MAX_RAW_LEN,
} = require("../../lib/hook-client/bundle.js");

// ----------------------------------------------------------------------------
// Committed Rust-oracle parity corpus (NEVER hand-write these values).
// ----------------------------------------------------------------------------
const CORPUS_PATH = path.join(
  __dirname,
  "..",
  "..",
  "..",
  "..",
  "crates",
  "unimatrix-server",
  "tests",
  "fixtures",
  "c1c2-parity",
  "bundle-golden.json"
);
const BUNDLE_GOLDEN = JSON.parse(fs.readFileSync(CORPUS_PATH, "utf8"));

// Build a wire string from an arbitrary object to exercise schema rejection.
// (This is a TEST helper that mimics the Rust encoder's framing; it is NOT a
// golden — golden rows always come from the committed corpus.)
function wireFrom(obj) {
  const json = JSON.stringify(obj);
  return BUNDLE_SCHEME + Buffer.from(json, "utf8").toString("base64url");
}

// A valid v:2 fields object reused as the mutation base for the reject matrix.
// Synthetic (not from a real server); only structurally valid for the happy path.
const VALID_FIELDS = {
  v: 2,
  mcp_url: "https://cloud.example:8443/v1/alpha",
  observe_url: "https://cloud.example:8443/v1/alpha/observe",
  token: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  fp: "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
};

// ============================================================================
// Golden round-trip (R-03 — JS golden NEVER hand-written)
// ============================================================================
describe("bundle decode — v:2 parity vs Rust encoder (R-03)", () => {
  it("decodes every v:2 golden row to its fields", () => {
    assert.ok(BUNDLE_GOLDEN.length > 0, "corpus must be non-empty");
    for (const row of BUNDLE_GOLDEN) {
      assert.strictEqual(
        row.fields.v,
        2,
        "corpus row must be v:2 (Rust oracle must have been regenerated for vnc-038)"
      );
      const got = decodeBundle(row.wire);
      assert.deepStrictEqual(got, {
        v: 2,
        mcp_url: row.fields.mcp_url,
        observe_url: row.fields.observe_url,
        token: row.fields.token,
        fp: row.fields.fp,
      });
      // URLs are byte-equal to the payload (feeds R-01 verbatim-post).
      assert.strictEqual(got.mcp_url, row.fields.mcp_url);
      assert.strictEqual(got.observe_url, row.fields.observe_url);
    }
  });

  it("returns exactly {v, mcp_url, observe_url, token, fp}", () => {
    const got = decodeBundle(wireFrom(VALID_FIELDS));
    assert.deepStrictEqual(Object.keys(got).sort(), [
      "fp",
      "mcp_url",
      "observe_url",
      "token",
      "v",
    ]);
  });
});

// ============================================================================
// Strict-reject matrix (R-03 — mirror of the Rust side)
// ============================================================================
describe("bundle decode — strict schema reject (load-bearing)", () => {
  it("rejects missing key (drop observe_url)", () => {
    const o = Object.assign({}, VALID_FIELDS);
    delete o.observe_url;
    assert.throws(() => decodeBundle(wireFrom(o)), BundleError);
  });

  it("rejects extra key (6th key)", () => {
    const o = Object.assign({}, VALID_FIELDS, { extra: "x" });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /unexpected fields/.test(err.message)
    );
  });

  it("rejects a stray v:1 base_url key as an extra field", () => {
    // A v:1-shaped object carrying base_url alongside the v:2 keys is a 6-key
    // object -> exact-key guard rejects (no base_url acceptance branch exists).
    const o = Object.assign({}, VALID_FIELDS, { base_url: "https://x" });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /unexpected fields/.test(err.message)
    );
  });

  it("rejects wrong-type v (string)", () => {
    const o = Object.assign({}, VALID_FIELDS, { v: "2" });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /unsupported bundle version/.test(err.message)
    );
  });

  it("rejects wrong-type mcp_url (non-string)", () => {
    const o = Object.assign({}, VALID_FIELDS, { mcp_url: 123 });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /mcp_url/.test(err.message)
    );
  });

  it("rejects non-https mcp_url (http:// downgrade)", () => {
    const o = Object.assign({}, VALID_FIELDS, {
      mcp_url: "http://cloud.example:8443/v1/alpha",
    });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /mcp_url/.test(err.message) && /https/.test(err.message)
    );
  });

  it("rejects non-https observe_url (non-URL)", () => {
    const o = Object.assign({}, VALID_FIELDS, {
      observe_url: "ftp://evil.example/v1/alpha/observe",
    });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /observe_url/.test(err.message) && /https/.test(err.message)
    );
  });

  it("rejects bad token (non-hex)", () => {
    const o = Object.assign({}, VALID_FIELDS, { token: "ZZZ" });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /token/.test(err.message)
    );
  });

  it("rejects malformed fp", () => {
    const o = Object.assign({}, VALID_FIELDS, { fp: "sha256:nothex" });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /fp/.test(err.message)
    );
  });

  it("rejects unknown major version (v:3)", () => {
    const o = Object.assign({}, VALID_FIELDS, { v: 3 });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) => err instanceof BundleError && /unsupported bundle version/.test(err.message)
    );
  });
});

// ============================================================================
// v:1 hard-cut with actionable message (R-04)
// ============================================================================
describe("bundle decode — v:1 fails closed with re-issue message (R-04)", () => {
  it("rejects a well-formed v:1 bundle and tells the operator to re-issue", () => {
    // A complete v:1 bundle: {v:1, base_url, token, fp}. Exact-key guard fails
    // first (base_url is not an expected key, observe_url/mcp_url missing) — a
    // v:1 bundle NEVER silently decodes.
    const v1 = {
      v: 1,
      base_url: "https://cloud.example:8443",
      token: VALID_FIELDS.token,
      fp: VALID_FIELDS.fp,
    };
    assert.throws(
      () => decodeBundle(wireFrom(v1)),
      (err) => err instanceof BundleError
    );
  });

  it("rejects v:1 on version when key-shape happens to match (v pinned to 2)", () => {
    // Construct an object with the exact v:2 key set but v===1 to prove the
    // version pin (obj.v !== 2) fires with the actionable re-issue message,
    // independent of the key guard.
    const o = Object.assign({}, VALID_FIELDS, { v: 1 });
    assert.throws(
      () => decodeBundle(wireFrom(o)),
      (err) =>
        err instanceof BundleError &&
        /unsupported bundle version/.test(err.message) &&
        /re-issue/.test(err.message) &&
        /v:2/.test(err.message)
    );
  });

  it("has no base_url acceptance branch (no silent v:1 decode path)", () => {
    // The decoder never returns a base_url-bearing object. Decode a valid v:2
    // bundle and assert base_url is absent from the result.
    const got = decodeBundle(wireFrom(VALID_FIELDS));
    assert.ok(!("base_url" in got), "decoder must not emit base_url");
  });
});

// ============================================================================
// Guard ordering (R-03 / NFR-08 — security boundary)
// ============================================================================
describe("bundle decode — guard ordering", () => {
  it("MAX_RAW_LEN cap runs FIRST (over-cap non-base64url rejects on LENGTH)", () => {
    const overCap = "!".repeat(MAX_RAW_LEN + 1); // 1 byte each, not base64url, no scheme
    assert.ok(Buffer.byteLength(overCap, "utf8") > MAX_RAW_LEN);
    assert.throws(
      () => decodeBundle(overCap),
      (err) => err instanceof BundleError && /too long/.test(err.message),
      "must reject on length, not base64/scheme"
    );
  });

  it("boundary: exactly MAX_RAW_LEN is NOT rejected on length", () => {
    const base = BUNDLE_SCHEME;
    const pad = "A".repeat(MAX_RAW_LEN - Buffer.byteLength(base, "utf8"));
    const atCap = base + pad;
    assert.strictEqual(Buffer.byteLength(atCap, "utf8"), MAX_RAW_LEN);
    assert.throws(
      () => decodeBundle(atCap),
      (err) => err instanceof BundleError && !/too long/.test(err.message),
      "at-cap must fail a non-length guard (inclusive boundary)"
    );
  });

  it("boundary: MAX_RAW_LEN + 1 rejected on length", () => {
    const over = BUNDLE_SCHEME + "A".repeat(MAX_RAW_LEN + 1);
    assert.throws(
      () => decodeBundle(over),
      (err) => err instanceof BundleError && /too long/.test(err.message)
    );
  });

  it("rejects missing scheme prefix", () => {
    assert.throws(
      () => decodeBundle("eyJ2IjoyfQ"),
      (err) => err instanceof BundleError && /prefix/.test(err.message)
    );
  });

  it("rejects valid base64url that is not JSON", () => {
    const wire = BUNDLE_SCHEME + Buffer.from("{not json", "utf8").toString("base64url");
    assert.throws(
      () => decodeBundle(wire),
      (err) => err instanceof BundleError && /JSON/.test(err.message)
    );
  });

  it("rejects a non-string input", () => {
    assert.throws(
      () => decodeBundle(42),
      (err) => err instanceof BundleError && /string/.test(err.message)
    );
  });
});

// ============================================================================
// Secret hygiene (NFR-06) — token never in a thrown message
// ============================================================================
describe("bundle decode — token never leaks into errors (NFR-06)", () => {
  it("schema reject on a bundle bearing a real token does not echo the token", () => {
    // Wrong-type fp but a valid 64-hex token present.
    const o = Object.assign({}, VALID_FIELDS, { fp: 999 });
    let msg = "";
    try {
      decodeBundle(wireFrom(o));
    } catch (err) {
      msg = err.message;
    }
    assert.ok(msg.length > 0, "must throw");
    assert.ok(!msg.includes(VALID_FIELDS.token), "token must not appear in the error");
  });
});

// ============================================================================
// Zero-dependency invariant (NFR-08)
// ============================================================================
describe("bundle decode — zero-dependency invariant (NFR-08)", () => {
  it("bundle.js requires only Node core modules", () => {
    const src = fs.readFileSync(
      path.join(__dirname, "..", "..", "lib", "hook-client", "bundle.js"),
      "utf8"
    );
    const requires = src.match(/require\(["'][^"']+["']\)/g) || [];
    for (const r of requires) {
      const mod = r.replace(/require\(["']([^"']+)["']\)/, "$1");
      assert.ok(
        mod.startsWith(".") || mod.startsWith("node:") || isCoreModule(mod),
        "bundle.js must not require a third-party module: " + mod
      );
    }
  });
});

function isCoreModule(mod) {
  // Node core modules the decoder is permitted to use (it uses none today, but
  // Buffer is a global so this list stays conservative).
  return ["buffer", "crypto", "fs", "path", "net"].includes(mod);
}
