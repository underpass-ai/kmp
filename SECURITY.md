# Security Policy

## Supported versions

Security fixes land on the actively maintained `main` branch and are shipped
in the next release. Only the latest published release is supported; older
versions should be upgraded before reporting a version-specific issue.

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

The local viewer binds to loopback by default. Treat an unexpected outbound
connection, a non-loopback viewer exposure, cross-project memory access or a
write outside the selected store as a security issue.
