## ADR-007: Container HTTP-Enable via `UNIMATRIX_HTTP_ENABLED=true` Env Var (resolves ARCHITECTURE §10 Q1 / C3 surface consistency)

### Context

The Unimatrix binary ships with the global default `http.enabled=false` — a UDS-only local install is the common case and must stay the default. The container, by contrast, exists to serve HTTPS. ARCHITECTURE §10 Q1 left the exact mechanism open: does the image flip the binary into HTTP-serving posture via an environment variable (`UNIMATRIX_HTTP_ENABLED=true`) or via a baked config file (e.g. `$UNIMATRIX_CONFIG` pointing at a committed `config.toml` with `http.enabled = true`)?

This is not merely a delivery detail — it touches the C3 env contract. C3 already establishes that the container's serving posture is configured through an environment variable: `UNIMATRIX_PUBLIC_URL` is the single knob feeding `derive_public_url()` (bundle base-url, allowed_hosts default, cert SAN). Whichever HTTP-enable mechanism is chosen sits directly alongside `UNIMATRIX_PUBLIC_URL` in the same `compose.yaml`, read at the same point in startup.

Constraints shaping the decision:
- The image is **distroless — no shell**. A config file baked into the image cannot be edited in place at runtime by an operator; changing a baked file means an image rebuild or a bind-mount overlay.
- The container serving posture must be **visible and overridable in `compose.yaml`** — the one file the operator actually touches.
- The global binary default `http.enabled=false` must stay clean — flipping it is a container concern, not a code-default change.

### Decision

Container HTTP-enable is the environment variable **`UNIMATRIX_HTTP_ENABLED=true`**, container-scoped, set in the image/`compose.yaml`. It is NOT a baked config file.

```yaml
# compose.yaml
services:
  unimatrix:
    environment:
      UNIMATRIX_HTTP_ENABLED: "true"
      UNIMATRIX_PUBLIC_URL: "https://cloud.example:8443"
```

- **Surface consistency (the load-bearing rationale):** the entire container serving posture stays on **one mechanism — environment variables** — alongside the existing `UNIMATRIX_PUBLIC_URL` (C3). An operator configures the cloud in one place, one way. A baked config file would split serving posture across two surfaces (file + env) for no benefit.
- **Greppable / visible in `compose.yaml`:** the operator sees the serving posture in the file they edit, not buried in an image layer.
- **Overridable without an image rebuild:** critical in a distroless image with no shell — an env var is set/changed in `compose.yaml` (or `-e` at `docker run`); a baked config file is not editable in place and would force a rebuild or bind-mount workaround.
- **Keeps the global binary default `http.enabled=false` clean:** the env var is container-scoped configuration, not a change to the code default. The binary's default posture is unchanged; the container layer flips it.
- **No secret concern:** a boolean is not sensitive, so an env var is the correct carrier. This does NOT extend to the token or cert, which remain files per NFR-05/NFR-06 (token at `{data_dir}/token` `0600`, cert/key at `{data_dir}/tls/` `0600`). Env carries the boolean knob; secrets stay on disk.

Precedence: `UNIMATRIX_HTTP_ENABLED=true` enables the HTTP listener regardless of the binary's `http.enabled=false` default (env is container-scoped override of the code default). TLS auto-detects the generated cert (provisioned first-boot per §4.1).

### Consequences

- **Easier:** Single configuration surface — both HTTP-enable and `UNIMATRIX_PUBLIC_URL` are env vars in `compose.yaml`; one mental model for the operator.
- **Easier:** Runtime override without rebuild — the operator flips serving posture in the file they already edit, even in a distroless no-shell image.
- **Easier:** Global binary default stays `false`; no code-default churn to make the container serve.
- **Harder:** One more env var the delivery must wire into config loading (read `UNIMATRIX_HTTP_ENABLED` as an override of `HttpConfig.enabled`). Bounded — it parallels the existing `UNIMATRIX_PUBLIC_URL` read in `load_config`.

### Related

- C3 (`UNIMATRIX_PUBLIC_URL`, `derive_public_url`): the sibling env var this decision aligns with — same surface, same mechanism (ARCHITECTURE §3 C3).
- NFR-05 / NFR-06: token and cert stay as `0600` files — env carries only the non-sensitive boolean, never the secrets.
- ARCHITECTURE §6 (Container integration row) and §2 (Container posture component): the env var is wired at the listener-gating point.
