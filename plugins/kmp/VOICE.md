# How KMP talks

One product, one voice. This file describes the product register. Workflow
rules live once, in `skills/*/SKILL.md`; Claude command files are thin host
adapters and must not copy those rules.

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
| `viewer could not bind: Address already in use (os error 98)` | `7317 was busy, so this session took its own free viewer port.` |
| `no startup recorded here yet` | `never started here. This is day one.` |

The pattern in every row: the left column reports on the software, the right
column talks to the person, in the same length or shorter.

## Ownership

The Rust binary owns machine state and lifecycle receipts. Skills own agent
behavior. Claude commands only route to a skill. Codex consumes the skills
directly. If a rule appears in two Markdown files, the extra copy is a bug.
