# Runbooks

Each runbook starts with read-only diagnosis, names what it changes and ends
with a verification step. For a single symptom, start with
[Troubleshooting](../troubleshooting.md).

| Situation | Runbook |
|:--|:--|
| The host cannot see `kmp_*` tools or startup fails | [MCP tools are missing](mcp-tools-missing.md) |
| A local store must be recovered, shared or moved | [Embedded recovery](embedded-recovery.md) |
| A shared Kubernetes release must be rendered, deployed or updated (experimental) | [Enterprise deployment](enterprise-deployment.md) |
