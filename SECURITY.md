# Security Policy

## Supported versions

The 1.x series receives security fixes for one year from the release of
1.0.0. Fixes ship in the next 1.x release and are not backported to earlier
ones: upgrade to the latest 1.x release before reporting a version-specific
issue. Surfaces marked experimental in
[Stability and support](docs/stability.md) get fixes on a best-effort basis.

## Reporting a vulnerability

Do not open a public issue for undisclosed vulnerabilities.

Use [GitHub private vulnerability reporting](https://github.com/underpass-ai/kmp/security/advisories/new).
If that form is unavailable, contact a repository maintainer through GitHub
without including exploit details in a public issue.

Please include:

- affected component
- reproduction details
- impact assessment
- any known mitigations

We will review reports as quickly as possible and coordinate remediation before
public disclosure when appropriate.

## Scope

This policy covers:

- the local `kmp-mcp` binary, viewer and embedded stores
- plugin installers, skills, host wiring and update flows
- MCP stdio and Streamable HTTP surfaces
- public gRPC and async contracts
- server bootstrap, container images, Helm charts and shipped adapters
- committed `.kmp/memory.jsonl` bundles and release artifacts

Product-specific integrations outside this repo should be reported to the
owning product as well when relevant.

## Local data boundary

Embedded KMP does not connect to a KMP service unless a remote endpoint is
explicitly configured. Its `.kernel/` store is local machine state and is
gitignored. The optional `.kmp/memory.jsonl` recovery bundle is plain,
reviewable project data intended to be committed; never write credentials or
secrets to memory.

The local viewer listens only once it is requested, and only on loopback: a
non-loopback address is refused. What can leave the machine, and when, is in
[Security and privacy](docs/security-privacy.md). Treat an unexpected outbound
connection, a non-loopback viewer exposure, cross-project memory access or a
write outside the selected store as a security issue.
