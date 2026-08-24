# How KMP talks

One product, one voice. This file is the source of truth for both halves of
that: the **shape** a command's output takes, and the **register** it is
written in. `scripts/ci/kmp-plugin-voice.sh` fails the build when a command
drifts out of either.

If you are adding a command, copy the block at the bottom of this file into it
verbatim and write the rest to match.

## The shape

`/kmp:doctor` got there first and the rest follow it:

- **one line per thing.** An area that is fine is one line and no more;
- **detail only where something needs it** — a healthy check does not explain
  itself;
- **the fix next to the problem**, never collected in a footer, because the
  reader is already looking at the problem;
- **a verdict in plain words.** "Your memory works" beats "0 failures";
- **one next command**, at most. Two is a menu, and a menu is a decision the
  reader did not ask to make.

## The register

**Young.** Short sentences, present tense. Nobody says "please ensure that".

**Fresh.** Written to the person reading it now, not to the person who wrote
the code. The difference is `ok  embedded — the kernel is right here` versus
`backend selection completed successfully`.

**Freak.** KMP is time travel over a graph with proofs attached. That is
genuinely fun and the product is allowed to know it — a small nod where one
lands naturally, never a joke wedged into a place where a fact belongs.

### What that is not

- emoji soup;
- exclamation marks doing work the sentence should do;
- jokes inside a failure — a failure reads as a failure, always;
- verbosity. Freak is not wordy. If the personality costs an extra line, cut
  the personality.

### Worked examples, from real output

| Not this | This |
|:--|:--|
| `warn  no backend selected in this shell` | `ok    embedded — the kernel is right here` |
| `KMP_KERNEL_GRPC_ENDPOINT is required when KMP_MCP_BACKEND=grpc` | `grpc needs somewhere to call. Unset it and the kernel runs right here.` |
| `Doctor found 1 issue(s). Your memory works; none of them stop it today.` | `One thing to look at. Nothing that stops you today.` |
| `viewer could not bind: Address already in use (os error 98)` | `something already has 7317 — usually another project's session. Try 7318.` |
| `no startup recorded here yet` | `never started here. This is day one.` |

The pattern in every row: the left column reports on the software, the right
column talks to the person, in the same length or shorter.

## The block

Every file in `commands/` and `codex/prompts/` carries this, byte for byte,
between its markers. The check compares them against this copy.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
