# Choosing the Trace mode and repairing a mixed call in one step

## Observed problem

After PR717, two native readers in an earlier reader evaluation mixed
`search.seek` with `paths_per_target`; one also added `select`. The catalogue
declared every field in one list, and the reduced card explained both uses one
after the other. The server refused only the first incompatible field: the
second reader needed two repairs. The solver did keep its candidates.

That reader also required equality between a verified copy and an authorized
publication. This was a condition added to the question: the same-action
example was applied to two different actions. Both readers left out the
observed cut in Trace. The original evidence, interviews and limits are in a
separate evaluation project (finding F-134); follow-up in
[#538](https://github.com/underpass-ai/kmp/issues/538) and
[#544](https://github.com/underpass-ai/kmp/issues/544).

## Decision

Keep one verb and the existing arguments. Announce two exclusive modes:

| Intent | Input | Own controls |
| --- | --- | --- |
| Connect destinations already known | `from` and `to` | `follow` or `direction`/`relations`, dimensions, `paths_per_target`, `select` |
| Discover proof without known destinations | `from` and `search.seek` | Roles with relation/direction/via/after/labels, `same_labels`, `same_ref` |

They share `max_nodes`, `max_edges`, `max_depth` and `max_states`. Clock and
pagination belong to the call; they are not a third mode. The mode follows from
the presence of `seek`: no redundant selector, no other language and no other
tool is added.

The schema declares a named `oneOf` with the list of fields each mode allows.
Each field's definition stays once in `properties`, with its mode stated. This
avoids duplicating large subschemas and keeps the current recursive validation
of unknown arguments. The generic validator does not change, and no schema
references that the host must resolve are introduced. The exclusion in the
schema helps clients that validate it; the native parser also enforces it,
without relying on the agent or the host to validate JSON Schema.

The parser checks the fields of mixed modes first and lists all of them in a
stable order, `to` included. It does not try to interpret an incompatible
`follow` before explaining the mix. The agent removes those fields in one
repair. This does not promise to list every shape, range or semantic error of
the request. A refusal runs no search, changes no data and replaces no
argument.

## Minimal teaching

The card starts by choosing one of the modes and gives a complete call with an
observed cut. The extended guide and the example tell these questions apart:

- "What does the copy verify and what does the publication authorize?" allows
  different actions. Do not add anchor equality; review the sources that
  connect them.
- "Is this same action verified and authorized?" requires equal anchors. A
  shared event or the same person does not identify an action.

An incompatibility requires reviewing the wording and the sources. Do not drop
equalities automatically to obtain a group. `review_required` is still a
pending review, and `as_of` is still a cut, not timestamp equality. A purely
historical question is answered with temporal navigation.

## Acceptance and control

1. Freeze the real failed calls, the valid calls and a destination control
   before editing code. Replay against PR717 and the candidate on copies of
   the same memory. Keep every attempt and source.
2. The first reader's original mix lists `select` and `paths_per_target` in a
   single error. Removing all the fields named produces the same valid
   request; the second reader's mix lists its only field. A mix with `to`
   lists it too.
3. The schema accepts both valid modes and refuses their mixes, as well as
   unions without `seek`. The parser keeps refusing unknown fields, also inside
   filters and destination options.
4. Valid calls keep selection, groups, candidates, relations, proof, clocks and
   continuations exactly. A real incompatibility of actions still returns zero
   groups; the different-actions case still keeps both candidates. The engine
   does not infer conditions from the question.
5. Regenerate the agent and human guides and the normal and Apps catalogues.
   Measure the catalogue, the card and the complete errors; do not confuse
   bytes with billed tokens, nor promise fewer agent errors without another
   independent trial.

Domain, protobuf, storage, scoring and search limits do not change. The signal
for a finished selection with paginated proof still pending is handled in
another increment under #544. The reader's sentence compression comes later.
A small change on integration, local checks and squash; no new editorial CI,
release or installation barrier.

## Local control result

Eleven frozen calls on copies of the same write: 13 RPCs per binary, 26 in
all, with no models. The five valid queries keep their `structuredContent`
exactly, including groups, candidates, relations and clocks. Six refusals per
side remain; the four mixes list all their incompatible fields in the
candidate. The JSON schema tells the valid modes from the mixes and still
refuses unions without `seek` and nested unknown fields. The readers were not
run again, and no understanding is attributed to this replay.

o200k_base measurement over compact JSON / literal text: Trace definition
4467 → 4712 tokens; audit card 726 → 661. The whole catalogue adds 245 tokens
and only Trace changes, both normal and with Apps. The first reader's first
refusal grows 317 → 380 tokens as it gathers its fields. This is not a saving
over a whole interaction, nor billed tokens. The operational sources were kept
in the separate evaluation project.
