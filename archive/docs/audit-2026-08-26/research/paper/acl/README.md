# ACL Submission Package

This directory contains an ACL-style LaTeX package derived from
[`archive/docs/research/PAPER_SUBMISSION_DRAFT.md`](../../../../archive/docs/research/PAPER_SUBMISSION_DRAFT.md).

The package is retained for the future recovery of the paper. It is not an
active submission and must be revalidated against the implementation and new
research results before publication work resumes.

The current manuscript covers four use cases:

- failure diagnosis with rehydration-point recovery
- why-implementation reconstruction
- interrupted handoff with resumable execution
- constraint-preserving retrieval under token pressure

## Files

- `main.tex`: ACL review-mode draft, with line numbers
- `main-preprint.tex`: ACL preprint-mode draft, without review line numbers
- `main.pdf`: compiled ACL review PDF (build locally, not committed)
- `main-preprint.pdf`: compiled ACL preprint PDF (build locally, not committed)
- `references.bib`: bibliography used by the draft
- `acl.sty`: official ACL style file vendored into the repo
- `acl_natbib.bst`: official ACL bibliography style vendored into the repo

## Style Files

The official ACL style files are already vendored in this directory. Source:

- https://github.com/acl-org/acl-style-files

## Build

From this directory:

```bash
pdflatex main.tex
bibtex main
pdflatex main.tex
pdflatex main.tex
```

For a readable local PDF without review line numbers:

```bash
pdflatex main-preprint.tex
bibtex main-preprint
pdflatex main-preprint.tex
pdflatex main-preprint.tex
```

The paper-use-case artifact used in the Results section is generated from the
repository root with:

```bash
bash scripts/ci/integration-paper-use-cases.sh
```

That script writes:

- `archive/artifacts/paper-use-cases/summary.json`
- `archive/artifacts/paper-use-cases/results.md`
- `archive/artifacts/paper-use-cases/results.csv`
- `archive/artifacts/paper-use-cases/results-figures.md`

`main.tex` defaults to anonymous review mode:

```tex
\usepackage[review]{acl}
```

`main-preprint.tex` uses:

```tex
\usepackage[preprint]{acl}
```

To switch to camera-ready mode later, replace it with:

```tex
\usepackage{acl}
```
