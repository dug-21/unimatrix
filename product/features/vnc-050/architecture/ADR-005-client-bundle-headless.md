## ADR-005: `client-bundle --headless` Extends the Existing Command and v:2 Bundle (Nullable fp, --trust)

### Context
Human constraint and SR-05: there is no parallel creation flow, no `token` command, and no new decoder. `client-bundle <slug>` (`client_bundle.rs`, vnc-038 ADR-002 #5081) emits the v:2 bundle `{v,mcp_url,observe_url,token,fp}` with the legacy token. `fp` is required to be `sha256:<64hex>` on both the Rust and JS decoders (`bundle.js`). OQ-7: a CA-issued front end (for example `tailscale serve` with a `*.ts.net` cert) needs a bundle with no pin, and the server cannot detect this because it only sees its own self-signed leaf. SR-06: the format must not drift silently. SR-18: a flag that is accepted but ignored must fail loud.

### Decision
- **CLI** (`ClientBundleArgs`, `#[derive(clap::Args)]` defined in `client_bundle/args.rs`; the `main.rs` variant becomes `ClientBundle(ClientBundleArgs)`):
  `client-bundle <slug> [--headless --mode read|read-write --label L] [--trust pin|ca] | --list | --revoke L`.
- **Flag matrix** (validated before any I/O; violations exit 2 with a usage message):
  - `--mode` or `--label` without `--headless` is an error. `--headless` without both is an error. There is no default mode.
  - `--list` and `--revoke` are mutually exclusive with each other and with `--headless`, `--mode`, `--label` and `--trust`.
  - `--trust` defaults to `pin` and is valid with or without `--headless`.
  - The `<slug>` must be registered for `--headless`, and must be valid for all forms.
- **Mint** (`--headless`): generate 32 OS-random bytes, append the entry under ADR-001's lock (a duplicate label is an error), and emit the bundle with the new token. Output contract is unchanged (#5088): stdout carries only the blob, and stderr echoes URLs, fp (or `trust: ca`), label, mode and slug. The secret is never printed outside the blob.
- **--list**: prints a stderr table of the tokens whose `allowed_slugs` contain `<slug>` (label, mode, slugs, created_at, revoked state; tombstones included). Hashes and secrets are never printed. stdout is empty.
- **--revoke L**: tombstones the record per ADR-001 (sets `revoked_at`, never deletes). Revoking a label that does not exist, one not bound to `<slug>`, or one already revoked is an error with no state change, never a silent success.
- **Label reuse**: `--headless --label L` fails (exit 1) if any record, active or revoked, holds `L` (FR-06a).
- **Bundle**: stays `v:2`. The only change is that `fp` becomes `string | null`, where `null` means CA trust (`--trust ca`). The exact key set is unchanged. Both decoders (`client_bundle::decode_bundle`, `bundle.js decodeBundle`) accept null and keep rejecting every other non-matching value. The parity corpus gains null cases. An old client that decodes `fp:null` rejects it loudly with "fp must be sha256", which is acceptable (fail loud, no silent downgrade). `init --bundle` accepts a CA bundle and stores `fingerprint:null`. The bridge now treats that as CA trust (ADR-006).
- Mode and label are **not** carried in the bundle. They are server-side authority.

### Consequences
- There is one creation flow and one decoder. `init --bundle` and `UNIMATRIX_BUNDLE` share the decoder.
- An operator who picks `--trust ca` for a self-signed server gets a loud TLS failure at attach time, not a silent pin bypass.
