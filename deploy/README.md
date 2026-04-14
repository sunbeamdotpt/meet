# deploy/ — kustomize stubs for `sbbb/base/meet/`

These manifests are written here and mirrored into
`sbbb/base/meet/` when the infrastructure lands (per PLAN.md §Infrastructure
and the one-at-a-time deploy rule in the studio CLAUDE.md).

## Secrets

Every `secretKeyRef` in this tree names a Secret that is **not** defined here.
Secrets are managed by `sbbb/` (ExternalSecret / VSO pointing at the
studio-wide secret store). Do not commit secret material to this repo.

The Implementer's env-var shapes come from DESIGN.md §4:

- `DATABASE_URL`, `VALKEY_URL`, `NATS_URL`
- `S3_ENDPOINT`, `S3_ACCESS_KEY`, `S3_SECRET_KEY`, `S3_BUCKET`
- `CALDAV_URL`, `CALDAV_USER`, `CALDAV_PASSWORD`
- `KRATOS_PUBLIC_URL`, `KRATOS_ADMIN_URL`
- `KETO_READ_URL`, `KETO_WRITE_URL`
- `LIVEKIT_URL`, `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`
- `SCALEWAY_API_KEY`, `MISTRAL_API_KEY`

## Layout

- `base/` — canonical Deployment, Service, Ingress, ConfigMap, ServiceMonitor,
  PrometheusRules.
- `overlays/dev/` — dev cluster patches (replica count, image tag, ingress
  host).
- `overlays/prod/` — prod patches.

Validate locally with:

```sh
kustomize build deploy/base
kustomize build deploy/overlays/dev
kustomize build deploy/overlays/prod
```
