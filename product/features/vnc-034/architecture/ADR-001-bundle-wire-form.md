## ADR-001: Connection Bundle Wire Form — `unimatrix-bundle:<base64url(json)>` (resolves OQ-A / C1)

### Context

C1 is the connection bundle: the artifact the server emits (`client-bundle`) and the client ingests (`init --remote`). It carries `{base_url, token, cert_fingerprint}` — cloud-wide, one bundle per cloud; the slug is appended per-project at client init, NOT part of the bundle. OQ-A asks the wire form: raw JSON, base64-of-JSON, or a single URL-with-fragment.

Constraints shaping the decision:
- The bundle is **copy-pasted by a solo developer** from server stdout into a client `init` command. It crosses terminals, chat, shell quoting, and clipboard managers. Raw JSON contains `"`, `{`, `:`, spaces — fragile under shell quoting and prone to truncation/whitespace mangling.
- It is a **trust boundary** (SR-09): the client parser accepts untrusted operator input and must schema-validate without ambiguity.
- The token is a 64-hex secret. A URL-with-fragment (`https://...#token=...`) risks the secret landing in browser history/referrer/logs if ever pasted into a URL-aware tool, and overloads URL semantics.
- The JS client must parse it with **zero dependencies** and a tiny code/size budget (< 250 KB total).

### Decision

The bundle wire form is a single line:

```
unimatrix-bundle:<base64url(canonical-json)>
```

- **Scheme prefix** `unimatrix-bundle:` — unambiguous self-identification; the client rejects anything without it. Distinguishes a bundle from a bare URL or token.
- **Payload** = base64url (RFC 4648 §5, URL-safe alphabet, **no padding**) of the canonical JSON:
  ```json
  {"v":1,"base_url":"https://cloud.example:8443","token":"<64-hex>","fp":"sha256:<64-hex>"}
  ```
- **Canonical encode**: fixed field order `v, base_url, token, fp`; no insignificant whitespace. Both sides encode identically (the server is the only encoder; the client only decodes, but the canonical form keeps fixtures stable).
- `v:1` is the schema version — the client rejects unknown major versions with a clear error (forward-compat seam).
- **Decode validates a strict schema**: exactly these four keys, `base_url` is `https://`, `token` is 64 lowercase hex, `fp` matches `^sha256:[0-9a-f]{64}$` (C2). Any missing/extra/malformed field is a hard parse error.

**Guard ordering (the guard runs before the work it prevents):**
1. **Length cap FIRST — on the RAW pasted string.** The 4 KB cap is enforced on the **raw pasted-string byte length BEFORE base64url-decode and BEFORE JSON-parse**. The cap must run before the work it prevents; decoding/parsing an unbounded paste is exactly the DoS the cap guards against, so it cannot run after. 4 KB keeps ≈10× headroom over a real bundle (a populated `unimatrix-bundle:` string is ~340 chars).
2. **Strict schema reject SECOND — the LOAD-BEARING guard.** For a pasted credential, the strict-schema reject (missing / extra / wrong-type field) is the **load-bearing** trust-boundary guard. The length cap is **belt-and-suspenders** — a cheap pre-filter against pathological input; it is the schema validation that actually establishes the bundle is well-formed and safe to act on.

base64url makes the whole bundle a single shell-safe, clipboard-safe token (alphanumeric plus `-` and `_`), eliminating quoting fragility while keeping a single decoder on each side.

### Consequences

- **Easier:** Copy-paste is robust — one opaque token, no shell quoting, no whitespace mangling, no fragment-in-URL secret leakage.
- **Easier:** One decoder, one schema validator on each side; the scheme prefix makes malformed input fail fast and legibly.
- **Easier:** Versioned (`v:1`) — future fields (e.g. an enterprise CA hint) are additive behind the version gate.
- **Harder:** The bundle is not human-readable at a glance (it is base64). Mitigation: `client-bundle` can also print the decoded fields to stderr for operator confirmation while emitting the encoded form to stdout.
- **Harder:** Adds a base64url encode on the server and decode on the client — trivial, zero-dependency, but a small amount of code on the size-budgeted JS side.

### Related

- C2 fingerprint format: ADR-002 (the `fp` field grammar).
- C3 public URL: the `base_url` field is `derive_public_url().base_url`.
- The slug is deliberately absent (C5 / ADR-004) — appended at `init`, not in the bundle.
