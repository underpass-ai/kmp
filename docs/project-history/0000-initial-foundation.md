# Initial foundation: first commit to the first merged PR boundary

## Boundary

- **Root commit:** [`3a3755b064c0816a477f074a3ba03d79694cc560`](https://github.com/underpass-ai/rehydration-kernel/commit/3a3755b064c0816a477f074a3ba03d79694cc560)
- **Root authored:** `2026-03-07T12:14:55+01:00`
- **Last pre-PR commit:** [`7756961794c8ce04bea735b514b02dbc8feee4c9`](https://github.com/underpass-ai/rehydration-kernel/commit/7756961794c8ce04bea735b514b02dbc8feee4c9)
- **Boundary authored:** `2026-03-07T12:42:55+01:00`
- **Next integration:** [`underpass-ai/rehydration-kernel` PR #1](https://github.com/underpass-ai/rehydration-kernel/pull/1)
- **Commits reachable at the boundary:** 5
- **Files at the boundary:** 47
- **Root-to-boundary tree delta:**  47 files changed, 2955 insertions(+), 1 deletion(-)

The first PR branched after the initial `main` foundation. This document stops
at its recorded base SHA, so PR #1's changes are not counted twice.

## What existed before PR #1

The pre-PR foundation was intentionally small. Its repository evidence shows:

- the initial product README and project identity;
- a Rust workspace scaffold with domain, application, port, adapter, transport,
  server, observability, and test-oriented crate boundaries;
- an initial split of protobuf contracts; and
- protobuf generation plus a CI quality gate.

The asynchronous command/projection contract was not yet integrated into
`main`; that change begins with the first PR dossier.

## Net capability additions

- A compilable Rust/Protobuf project skeleton for an API-first memory kernel.
- Hexagonal boundaries that subsequent PRs could fill without coupling the
  protocol to a persistence or transport implementation.
- A generated-contract and CI baseline capable of detecting schema or quality
  regressions before later integrations.

## Net capability removals

No capability removal is evidenced in this pre-PR interval. The commits build
the initial repository from an empty root rather than replacing a prior
in-repository implementation.

## Evidence commands

```bash
git log --reverse 7756961794c8ce04bea735b514b02dbc8feee4c9
git diff --stat 3a3755b064c0816a477f074a3ba03d79694cc560 7756961794c8ce04bea735b514b02dbc8feee4c9
git ls-tree -r --name-only 7756961794c8ce04bea735b514b02dbc8feee4c9
```

This initial document uses the Git graph rather than a PR description because
no pull request exists for the interval.
