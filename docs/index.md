# KMP documentation

Start with the repository [README](../README.md). It covers normal local use,
privacy, installation and interaction with an agent.

## Choose a topology

| Need | Documentation |
|:--|:--|
| One developer or repository; no service to operate | [Embedded KMP](embedded/README.md) |
| One live memory shared across machines, agents or services | [Enterprise KMP](enterprise/README.md) |

Embedded is the default. Enterprise is optional and self-operated.

## Common tasks

- [Configure local retrieval](embedded/configuration.md) — defaults, opt-ins and how to read Store config.
- [Recover missing tools](runbooks/mcp-tools-missing.md) — engine, host ownership and stale sessions.
- [Recover embedded memory](runbooks/embedded-recovery.md) — preserve a store and restore a portable bundle.
- [Use the shipped guides](../plugins/kmp/guide/README.md) — progressive agent guidance and the human ChronoLoom path.

## Other work

- [Architecture](architecture/README.md) — components, data flows and trust boundaries.
- [Runbooks](runbooks/README.md) — diagnosis, recovery and enterprise deployment.
- [Development](development/README.md) — tests, contracts and releases.
- [Research](research/README.md) — current questions and retained publication work.
- [Documentation catalog](documentation-catalog.md) — authority and history boundaries.

The complete documentation tree was restarted on 2026-08-26. The previous
tree remains available as untrusted audit material in Git history.
