# KMP in three claims

This is the public pitch, kept deliberately narrow. Every claim below points
to a terminal recording produced by the current binary, and all recordings
regenerate with one command:

```bash
bash scripts/demo/record-pitch.sh
```

The `.tape` files are the editable source. The shell scenarios underneath
them execute KMP rather than printing canned transcripts; CI runs those same
scenarios and checks the observations that make each claim true.

## 1. One binary, no service behind it

An unconfigured `kmp-mcp` opens the embedded engine and exposes all ten tools.
No database service, account, or API key is involved.

![One binary starts the embedded KMP engine](recordings/one-binary.gif)

Source: [`one-binary.tape`](tapes/one-binary.tape) · scenario:
[`one-binary.sh`](../../scripts/demo/pitch/one-binary.sh)

## 2. A fresh process recovers the memory and its proof

One process writes a decision and exits. A second recovers the decision; a
third inspects the evidence. The processes share only the data directory.

![Memory and evidence survive fresh processes](recordings/fresh-session.gif)

Source: [`fresh-session.tape`](tapes/fresh-session.tape) · scenario:
[`fresh-session.sh`](../../scripts/demo/pitch/fresh-session.sh)

## 3. Wrong turns remain inspectable

The example incident keeps the rollback that looked right at 15:05, the later
evidence that disproved it, and the reason for the replacement decision. The
same store is visible through the bundled local viewer.

![A wrong turn remains beside the evidence that contradicted it](recordings/wrong-turn.gif)

Source: [`wrong-turn.tape`](tapes/wrong-turn.tape) · scenario:
[`wrong-turn.sh`](../../scripts/demo/pitch/wrong-turn.sh)

## Contract

[`claims.tsv`](claims.tsv) is the machine-readable claim-to-recording map.
`scripts/ci/pitch-material.sh` refuses an unmapped claim, a missing recording,
or a scenario that no longer proves its expected observation.
