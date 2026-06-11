# Cert Rotation Runbook — Remote Unimatrix Server

> Operator procedure for rotating the server's TLS certificate. Required deliverable (vnc-034 / FR-A11, AC-CT-ROT).

Unimatrix clients pin the server's TLS certificate by **fingerprint** (`sha256:<hex>` over the served leaf certificate — ADR-002). There is no CA trust and no hostname validation; the pinned fingerprint *is* the trust model. This means rotating the certificate **invalidates every existing client pin** until each client re-pins.

There is no rotation tooling and none is needed: the existing `client-bundle` + `init --remote` flow **is** the rotation flow. Rotation is three steps.

---

## When to rotate

- The certificate is expiring or has expired.
- The private key may be compromised.
- You are changing the server's public hostname (the SAN), which changes the cert.

## What stays the same vs. changes

| Value | Changes on rotation? |
|-------|----------------------|
| Bearer token (`{data_dir}/token`) | **No** — unchanged. Rotating the cert does not rotate the token. |
| Public base URL (`UNIMATRIX_PUBLIC_URL`) | **No** (unless you are intentionally moving hosts). |
| Cert fingerprint (`fp` in the bundle) | **Yes** — a new cert means a new `sha256:` fingerprint. This is the value clients must re-pin. |

---

## Procedure (three steps)

### Step 1 — Rotate the certificate on the server

Replace the cert/key, or delete them to trigger first-boot regeneration, then restart the container:

```bash
# Option A — regenerate (self-signed): delete the cert/key and restart.
#   The Rust binary re-provisions a fresh cert/key on next boot.
docker run --rm -v unimatrix-data:/data busybox \
  rm -f /data/.unimatrix/<hash>/tls/cert.pem /data/.unimatrix/<hash>/tls/key.pem
docker compose restart

# Option B — install your own cert/key:
#   write the new PEM files to {data_dir}/tls/cert.pem and {data_dir}/tls/key.pem
#   (the private key file must be mode 0600), then restart.
docker compose restart
```

`{data_dir}` is `/data/.unimatrix/<hash>` (the hash is derived from `--project-dir /data`). The cert/key live under `{data_dir}/tls/`. After restart the server is serving the **new** certificate; the new fingerprint differs from the old one.

### Step 2 — Re-run `client-bundle`

Generate a fresh connection bundle. It carries the **new** fingerprint with the **unchanged** base URL and token:

```bash
docker compose exec unimatrix unimatrix --project-dir /data client-bundle
```

- **stdout** is the opaque `unimatrix-bundle:<...>` blob (pipe it; the token never appears in plaintext).
- **stderr** echoes the `base_url` + `cert-fingerprint` with the **token redacted** — confirm the fingerprint changed and the base URL is your real host (not the `<EDIT-ME>` placeholder).

### Step 3 — Re-`init` every client

Distribute the new bundle and re-pin each client:

```bash
# On each client machine:
unimatrix init --remote 'unimatrix-bundle:<new-blob>'
```

Each client re-pins to the new fingerprint and reconnects. No token change is required — `init --remote` overwrites the stored fingerprint and endpoint.

---

## What a client sees if you skip Step 2/3 (diagnosable mismatch)

If the cert is rotated but a client is **not** re-bundled, that client is still pinned to the **old** fingerprint. On its next connection it surfaces a **clear, diagnosable fingerprint-mismatch error** — not an opaque TLS handshake failure. The error names the **expected** (pinned) and **presented** (served) `sha256:` fingerprints and points you back to this runbook:

```
TLS pin mismatch: the server certificate changed.
  expected (pinned):  sha256:<old-hex>
  presented (server): sha256:<new-hex>
The server certificate was rotated. Re-run `client-bundle` on the server and
`init --remote <new-bundle>` on this client to re-pin.
```

This legibility is by design (ADR-002 + the client's `checkServerIdentity` pin check) — it is what makes rotation a three-step fix instead of an opaque outage.

---

## See also

- `docs/client-setup.md` — initial client attach over pinned TLS.
- `docker-compose.yml` — `UNIMATRIX_PUBLIC_URL` / `UNIMATRIX_HTTP_ENABLED` serving posture (ADR-007).
