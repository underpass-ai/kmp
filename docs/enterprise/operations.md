# Enterprise deployment and operations

The supported assembled deployment is the Helm chart at
[`distribution/charts/kmp`](../../distribution/charts/kmp/). The OCI image at
`ghcr.io/underpass-ai/kmp` contains `kmp-server`, `kmp-mcp-http` and the
reference gRPC client; its default entrypoint is `kmp-server`.

## Required decisions

Before rendering a production release, choose:

1. an immutable image tag or digest;
2. externally operated or chart-managed development instances of Neo4j,
   Valkey and NATS;
3. a Kubernetes Secret containing all connection URIs;
4. disabled, server TLS or mutual TLS for kernel gRPC;
5. whether the optional authenticated HTTP MCP gateway is required;
6. an OTLP destination and log collection policy;
7. persistence, backup and recovery policies for every stateful dependency.

`values.yaml` is the configuration schema. The chart rejects missing images,
mutable `latest` outside explicit development mode, and inline connection URIs
outside explicit development mode.

## Validate the chart

```bash
bash scripts/ci/helm-lint.sh
```

For a disposable render using the repository's development values:

```bash
helm template kmp distribution/charts/kmp \
  --namespace kmp \
  -f distribution/charts/kmp/values.dev.yaml
```

Development values are not production defaults.

## Deploy explicit production values

```bash
helm template kmp distribution/charts/kmp \
  --namespace kmp \
  --values <your-values.yaml> \
  --set image.tag=vX.Y.Z

helm upgrade --install kmp distribution/charts/kmp \
  --namespace kmp --create-namespace \
  --values <your-values.yaml> \
  --set image.tag=vX.Y.Z \
  --atomic --wait
```

Do not reuse `values.underpass-runtime*.yaml` unchanged: those files describe
one Underpass environment, not a portable production policy.

The repository deployment workflow calls
[`scripts/ci/deploy-kubernetes.sh`](../../scripts/ci/deploy-kubernetes.sh),
which requires an image tag or digest and supports server-side dry runs.

## Verify

- render and lint the chart;
- run [`scripts/ci/kubernetes-transport-smoke.sh`](../../scripts/ci/kubernetes-transport-smoke.sh)
  for the gRPC transport boundary;
- run [`scripts/ci/mcp-http-live-smoke.sh`](../../scripts/ci/mcp-http-live-smoke.sh)
  when HTTP MCP is enabled;
- enable and run Helm tests only against an environment intended for live
  validation;
- verify application behavior before switching agent endpoints.

The chart's values and templates are authoritative. Archived runbooks are not.

## Data movement

Do not copy local embedded-store files into enterprise storage. Export the
local event bundle, plan canonical ingest, verify about coverage, temporal
ordering, relations and evidence in the deployed kernel, and retain the source
until the migration is accepted.

## Server data directory

The kernel keeps one file of its own: `telemetry-salt`, the random key of the
Ask and Wake log fingerprints, in `KMP_DATA_DIR` (the chart mounts
`serverData.mountPath`, `/var/lib/kmp`). It is created on first use with mode
0600, is never logged and is not memory: the memory stays in Neo4j, Valkey and
NATS. `serverData.persistence` (on by default, 64Mi, `ReadWriteOnce`) keeps it
across restarts so fingerprints stay comparable; with it off, each pod starts a
new salt. With `replicaCount > 1`, give it a `ReadWriteMany` class. Rotating the
salt is deleting the file; fingerprints before and after stop comparing.
`KMP_TELEMETRY_SALT_PATH` overrides the location. Outside the chart the default
is `$XDG_DATA_HOME/kmp/server` (`~/.local/share/kmp/server`).

## Upgrades and rollback

Pin releases, render the exact values, and use Helm's atomic upgrade behavior
where appropriate. Application rollback does not replace backup and recovery
plans for Neo4j, Valkey and NATS. Released images and charts are immutable;
publish a new patch instead of moving a release tag.
