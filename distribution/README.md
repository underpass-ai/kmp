# Distribution

Release and deployment assets that ship KMP live here.

- [`charts/kmp/`](charts/kmp/) — the current optional Kubernetes Helm chart;
- [`lexical-bridge/`](lexical-bridge/) — the lexical-bridge table every
  release publishes and `kmp-mcp setup` installs, with its checksum and its
  provenance;
- [`mcpb/`](mcpb/) — the packaged MCP distribution.

Historical standalone Kubernetes manifests remain available in Git history
and are not a supported deployment path. See
[Enterprise KMP](../docs/enterprise/README.md) for the current shared KMP
topology.
