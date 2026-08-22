# Pull-request history agent

This directory is an evidence-backed evolution record, not a changelog written
from memory. Maintain both repositories in the KMP code lineage:

1. `underpass-ai/rehydration-kernel` (archived predecessor)
2. `underpass-ai/kmp` (current repository)

## Required workflow

1. Run `python3 scripts/pr_history.py sync` from the repository root. The tool
   paginates the GitHub Pulls API, selects every merged PR, and reconstructs
   each first-parent integration diff from the local Git graph.
2. Review new or changed dossiers one PR at a time. Read the PR description,
   integrated commits, changed paths, relevant production code, tests, public
   contracts, and documentation. Do not infer behavior from the title alone.
3. Keep the four claims separate:
   - capability added;
   - behavior, maintenance, or contract changed;
   - capability removed;
   - compatibility deliberately preserved.
4. A removal claim requires positive evidence in the integrated diff, a test,
   a contract change, or an explicit integrated commit. File deletion alone is
   not proof that a user-visible capability disappeared.
5. Treat the PR body as author intent and the first-parent tree diff as the
   integration fact. If they conflict, record the conflict; never silently
   repeat the body as repository truth.
6. Do not create files for numeric gaps. GitHub issues and PRs share a sequence,
   so a missing PR number is not a missing pull request.
7. Run both checks before finishing:

   ```bash
   python3 -m unittest scripts.tests.test_pr_history
   python3 scripts/pr_history.py validate
   ```

## Completeness invariants

- Exactly one initial-foundation document ends at the base SHA of the earliest
  merged PR in the lineage.
- Exactly one dossier exists for every merged PR returned by GitHub.
- Repository slug plus PR number is the identity. Numbers reused after the
  repository transition must never overwrite predecessor records.
- Every merge SHA exists locally and is reachable from `main`.
- Every dossier carries its audited first-parent range, commit list, changed
  paths, and line statistics.
- The index count and links match the dossiers on disk.

## Editing rule

The sync command owns metadata, source descriptions, and Git-derived evidence.
If deeper semantic conclusions are added, place them in a clearly labeled
`Curated analysis` section and preserve their supporting paths, symbols, tests,
or contract references. Re-run sync carefully: generated sections may be
replaced, so preserve reviewed analysis before regeneration.
