# Updating KMP's agent-facing surface

This procedure is for whoever maintains KMP, a person or an agent. The surface
is what an agent receives when it connects, the tools it discovers, the skills
it invokes, the guide it consults and the answers it uses to choose its next
move. Changing one of these parts may require updating the others.

The result must let an agent use the capability with evidence and at a known
context cost. A shorter description is not an improvement if it causes wrong
calls, loses proof or confuses the clocks.

The per-increment notes that used to follow this procedure (Trace routing,
seek, proof materialization, read notices, Condense candidates) are archived in
[agent-surface-increments.md](../archive/development/agent-surface-increments.md).

## Where each thing changes

Paths are relative to the repository root.

| What changes | Source to edit | What must go with it |
| --- | --- | --- |
| Kernel behaviour | Domain, use cases and adapters that implement the operation | A behaviour test and the matching MCP contract |
| A tool's arguments, results and description | [contract/tools](../../crates/kmp-mcp/src/contract/tools/) and [contract/schema](../../crates/kmp-mcp/src/contract/schema/); registered in [registry.rs](../../crates/kmp-mcp/src/contract/registry.rs) | Surface fixture, the verb's guide and the affected example |
| The explanation of an argument | The schema `description` in [contract/tools](../../crates/kmp-mcp/src/contract/tools/). `tools/list` strips it (except the short lines in `ADVERTISED_LINES`) and `kmp_guide` serves it in the `parameters` of the topic that owns that path, following `serves` in [guide_topic.rs](../../crates/kmp-mcp/src/guide/guide_topic.rs) and the projection in [parameter_guide.rs](../../crates/kmp-mcp/src/contract/parameter_guide.rs) | Every new tool must belong to a topic. `parameter_guide_audit` checks that exactly one topic serves each argument, and `tests/guide_topics.rs` that no card exceeds 2,000 tokens |
| What `tools/list` publishes of each schema | [published_law.rs](../../crates/kmp-mcp/src/contract/published_law.rs): types, enums and `required`; the rest lives in the full contract the validator reads | `published_law_audit` builds, for each unpublished constraint, a call that breaks only that one and checks the refusal; `tests/schema_contract_fuzz.rs` fuzzes the published schema, the closed objects and the documented rules |
| A rule across arguments (`if`/`anyOf`/`oneOf`…) | The conditional is tagged with `cross_field_rule::tagged(id, …)` in the definition; its wording and its fix are in [cross_field_rule.rs](../../crates/kmp-mcp/src/contract/cross_field_rule.rs) | The card of the topic that owns the argument serves it under `rules`; the refusal names it (``Rule `id` `` and `feedback[].rule`); an untagged conditional fails `published_law_audit` |
| A tool's MCP annotations | `ToolEffect` in [definition.rs](../../crates/kmp-mcp/src/contract/schema/definition.rs) | `tests/read_only_tools.rs`: a read leaves the store and its sidecars byte-identical; an idempotent writer does not write on the repeat; `chatgpt-app-submission.json` carries the same values |
| Rules common to every connecting agent | [agent_policy/instructions.rs](../../crates/kmp-mcp/src/agent_policy/instructions.rs), served by [handshake.rs](../../crates/kmp-mcp/src/contract/handshake.rs) | Routing cases, and captures of `initialize` and of the catalogue the host exposes |
| The relation vocabulary | `KnownMemoryRelationType` and its specification in [relation_type.rs](../../crates/kmp-domain/src/value_objects/relation_type.rs) | The projection [relation_vocabulary.rs](../../crates/kmp-mcp/src/contract/schema/relation_vocabulary.rs), positive and negative cases, and teaching of why/evidence |
| The agent's short entry | [guide/agent-entry.md](../../plugins/kmp/guide/agent-entry.md) | Regenerate the index and check its size; do not paste the whole manual in |
| Extended explanation or example | [editorial.json](../../plugins/kmp/guide/editorial.json) and its `text_file` entries in `verbs/`, `topics/` or `examples/` | A valid reference, a native test and selective reading |
| Mapping from tool to verb | `tool_verb` in [GuideRequestMapper](../../crates/kmp-release/src/application/mappers/guide_request_mapper.rs) | Every public tool must resolve to an indexed verb |
| Help on a usage refusal | [tool_error_help.rs](../../crates/kmp-mcp/src/serving/tool_error_help.rs) and its wrapper in [tool_result.rs](../../crates/kmp-mcp/src/serving/tool_result.rs): one line `Help: kmp_guide {"topic":…}` | Choose the topic by the typed path of the feedback, keep the feedback, and check that the topic exists |
| How Codex or Claude enter | [skills](../../plugins/kmp/skills/), [Claude adapters](../../plugins/kmp/claude/commands/) and [capabilities.json](../../plugins/kmp/capabilities.json) | Capability parity and access to the same entry; avoid another copy of the manual |
| The product's public story | The `kmp:public-overview` block of [plugins/kmp/README.md](../../plugins/kmp/README.md) | Sync the repository and crate READMEs; review the human guide |
| Installing the stored guide, and the demo | [guide/asset_locator.rs](../../crates/kmp-mcp/src/guide/asset_locator.rs) (where an installed guide may be), [serving/guide_repair.rs](../../crates/kmp-mcp/src/serving/guide_repair.rs) (the `GUIDE_UNAVAILABLE` repair), `kmp-mcp guide sync`, [demo.rs](../../crates/kmp-mcp/src/demo.rs) and its packet in `fixtures/demo/` | Tests `guide_sync`, `guide_repair` and `demo_cli`; `verbs/guide.md`, the guide and setup skills, and `examples/retry-budget` regenerated with `scripts/examples/refresh.sh`. No read seeds the guide: `kmp_guide` serves its cards from the engine, and `kmp_inspect` of a guide node the store lacks answers `GUIDE_UNAVAILABLE` |

## Adding a verb: what `kmp_condense` needed

A new verb does not end in `contract/tools/` or in the registry. This is the
whole path of the last one added, as a reference for what is touched and why,
not as a list a CI should check.

| Piece | Why |
| --- | --- |
| `contract/tools/condense.rs` and `contract/registry.rs` | The tool and its place on the surface |
| `serving/adapters/embedded_backend.rs`, `serving/adapters/grpc/tools.rs` | The verb exists in both backends or it does not exist |
| `serving/adapters/fixture_backend.rs` and its response fixture | The examples backend answers everything it announces. It is compiled only in debug builds and with the `eval-hooks` feature |
| `tool_error_help.rs` | A refusal returns its own verb and its own lesson; pointing at another move's lesson answers worse than not answering |
| `guide_request_mapper.rs::tool_verb` | Every public tool resolves to an indexed verb |
| `guide/guide_topic.rs`, `guide/cards/` (the editorial source and the compiled copy in `crates/kmp-mcp/src/guide/cards/`) and its link to the verb | The topic exists in the index and serves a card compiled into the engine; indexing the verb alone does not make it discoverable |
| `guide/editorial.json` and its `text_file` | The canonical source of the guide; regenerate with `guide assets write` |
| `plugins/kmp/capabilities.json` | The generator compares the live list against this inventory and fails if they differ |
| `distribution/mcpb/manifest.json` | What the host installs |
| The `kmp:public-overview` block and `readme sync` | The three public surfaces describe the same surface |
| `tests/tool_surface_parity.rs` | Pins the definition and the response |
| `plugins/kmp/skills/kmp-moves/SKILL.md` | The skill that lists the live moves |
| The ownership table and the list of moves in the root README | They describe the same surface |

Not every verb has a gRPC binding. `kmp_summaries_audit` reads the store's own
event log, earlier revisions of each entry included, and the typed service
does not expose that stream. `kmp_curate` has no gRPC binding either. In live
gRPC mode both answer `unavailable` and say so
(`crates/kmp-mcp/src/serving/adapters/grpc/tools.rs`); their descriptions and
the crate README's table state it. Such a verb still needs the examples
backend, a response fixture and parity. What it must not do is answer a weaker
read under the same name from the projections.

Fixed counts ("sixteen tools", "twelve memory tools") were removed from prose
where they added nothing: each new verb left them stale in four places at once.
Where the list helps (the moves skill and the README table) the list stays and
points at `tools/list` as the authority. There is no count test and no new
gate.

The `kmp_condense` response is not pinned in that parity test: a card declares
the digest of the record the store holds, and that value differs between
seeded stores. The positive call is covered with a dynamic descriptor in
`kmp-mcp/tests/reader_cards.rs` and `kmp-transport-grpc/tests/condense_parity.rs`,
which read the descriptor from the store and write against it over MCP and over
gRPC. If someone resolves the digest at call time inside the parity test, the
exception documented there is no longer needed; taking the tool out of the
loop is not the alternative.

When measuring the cycle, keep the quantities apart:
`search.max_body_record_bytes` bounds `Details` records, not the total size of
the response nor the process's memory. Response bytes are not tokens nor peak
RAM, and a card shorter than its body does not guarantee that a whole cycle
pays back.

`guide/AGENT.md`, `guide/guide.requests.json` and `guide/memory.jsonl` are
generated outputs. Do not correct them by hand: correct the source and
regenerate all three. The contract fixtures are reviewable evidence; they are
not updated just to make a test pass that caught an unexpected change.

## 1. Define the change from its use

Describe the case that changes and the observable result before and after.
Identify what the agent must learn, which argument it uses, what KMP returns
and what proof lets it trust that. Include the temporal and relational path
when the task requires walking a history; an Ask example does not cover that
use.

Mark the affected surfaces with the table above. If only an explanation
changes, kernel behaviour need not change. If a tool is added, also review the
inventory, the registry, the guide mapping, the fixtures,
`distribution/mcpb/manifest.json` and real discovery in the host. The manifest
lists the tools of the package the host installs; check it with
`bash scripts/ci/mcp-registry.sh`. Avoid repeating a fixed count in several
tests.

## 2. Put the explanation where it is looked up

A write completes an omitted class only if the domain's `writer_spec` allows a
single class. When changing that vocabulary, also review the relations that
require `class` in the schema and their `allowed_values` feedback; both are
projected from the domain. An explicit class is never replaced. Local ids,
with or without `@`, resolve only exact matches inside the packet; do not turn
local errors into advice to link other abouts. Check future names, unknown
names, canonical refs, proof, receipt and replay after a restart. Keep the
packet's explicit observation: completing the class does not authorize
inferring clocks or choosing the memory kind from the prose.

Keep the short entry as a map. A verb's detail must explain when to use it,
when to choose another, the minimal input, the result, errors or limits and the
next move. Skills point to that entry and reuse what was already read; they do
not reload the whole guide for each operation.

In `editorial.json`, each entry has a stable `id` and **one** source: `text` or
`text_file`. `guide_title` includes it as a verb or topic in the index;
`example_title` includes it as a lesson. The builder keeps the body as a KMP
node and generates its full references. The reader copies those references; it
does not rebuild them. Keeping the `id` of an existing concept and regenerating
its content avoids breaking references because of a wording change.

A new example must contain explicit sources, justified write decisions, calls
with returned refs, the expected result and limits. Keep available sources
apart from future questions. Include a counter-example where a confusion is
likely: sharing a label does not prove identity; occurred yesterday does not
mean observed yesterday. A written replay demonstrates a contract; it does not
show that another LLM learned to use it.

On a write, consume the whole `feedback` list: each record that fails during
independent compilation contributes its first error, with code, path and action
when there is one. Keep those fields when grouping; do not rebuild repairs from
the general message. The packet's shape, its identities and backend failures
may stop validation earlier. Nor is it an exhaustive list of every error of one
record. Check that the complete refusal writes nothing and that the actions are
still executable after grouping.

The writer's self-check must compare the sources with what it actually stored:
observation clock against occurrence, validity, proof and a useful dimensional
path. Reuse the context already available and widen only what is needed.
Packet coverage does not show source coverage; the literal lint of
`summary_en` does not check semantic equivalence. The identifier comparator
recognizes full dates with the month written in Spanish or English, and
YYYY-MM-DD. A date with a written month and no year keeps month and day as
`--MM-DD`; a full date in the rendering may cover those components, without the
lint validating the added year. Teach keeping the year absent when the
evidence does not justify it. A full source date never loses its year. Other
formats keep literal comparison. Test that a number unrelated to the date is
not covered by it, and that stored texts and retrieval tokens stay intact.
Teach concrete examples of these differences, stable keys for the same path
and facts with separate lifecycles. Do not impose quotas of kinds or labels,
nor turn this informative review into an editorial gate.

When changing the representation of a continuation, check the complete
reconstruction from its pages against an unpaged read. In Inspect,
`page.repeat_object=false` works only with a cursor and the initial object
kept; `object_reused=true` and `object.ref` identify what was reused. Text,
metadata, source and proof stay bound to the cursor even when not repeated.
Inspect returns in `next_actions` a handle to the complete call. If one item
does not fit, its budget offers at least `page.minimum_progress_bytes`; it
prefers the complete inspection when that fits in the usual 10,000 bytes, to
avoid one-item-per-call retries. Keep the meaning of `required_bytes`: the
complete inspection with its object. Count the action and the warning in the
response floor, and run the retry to check that it advances. A conflict
returns in feedback a restart without `page`, which recovers the new object
even if earlier pages reused it. Verify the refusal if they change, and measure
bytes and calls of the whole journey, first page included; leaving out a
repeated body does not authorize summarizing or removing evidence.

Wake and Ask send their stable core only once: a continuation (`page.cursor`
past the first page) returns only the new expansion items, sets
`projection.core_reused=true` and does not repeat the summary, answer, `scope`,
pinned proof or core warnings. `page.repeat_core=true` returns it again for the
host that lost the first page; the action that answer proposes goes back to the
incremental continuation. `projection` is the only progress block (the former
`truncation` repeated its counters). Test that the first page plus every
continuation reproduces the complete read with a large budget, and that the
cursor still refuses another selection with and without `repeat_core`.

An Ask the gate leaves `answered` offers no continuation below
`budget.detail: "full"`: its first page carries the cited core and whatever
fits, `projection.more_on_request` counts the rest (field 12 of
`RecallProjection`) and `next_action` is null. If the core was shortened, the
restart proposes only the margin that fits the whole core. `partial` and
`unknown` keep paging. A continuation, and the restart of a shortened page, are
cut from the frozen read of the first page: the echo of the request does not
name a `detail` the caller did not name, because for the core `balanced` and
"no level" are different queries.

In an Ask, the stable core carries from `proof.superseded` only the
supersessions that touch a cited memory (the superseded one or the one that
supersedes). The rest is expansion of the `proof.superseded` section, behind
the ordered evidence: it pages like any other and, on an `answered` first page,
counts in `projection.more_on_request`. A Wake keeps all its marks in the core,
and the section appears in `projection.sections` only when it pages something.
A `reached_by: lifecycle` item is expansion like the other rescues: it goes on
the first page of an `answered` if it fits and, if not, counts in
`more_on_request`.

When changing temporal labels, test an old entry with a later membership: the
membership cannot satisfy a selector nor appear in the coordinates of the
earlier Goto. Keep multiple labels and filters of other dimensions (#560), an
absent clock, inclusive and exclusive ends, and continuations. The test keeps
antecedents earlier than the interval, but their memberships respect its upper
cut. This check is behavioural, not editorial.

In the temporal verbs, `page` counts items of entries and proof of the same
packet; `selection` identifies the core's bounded selection. Run `next_actions`
unchanged: the page continuation is a handle to the complete call and
navigation repeats its call. First rebuild that packet, then navigate the
history that remains outside. Do not turn `page.next_cursor` into a memory
reference. A continuation test must keep the filters, the clock and the proof,
and check that the cursor refuses changes to the content. Measure the whole
journey: four pages may cost more tokens than one large answer, even though
they allow progress with a smaller limit per response.

When changing `fields`, check what is projected: in the temporal verbs only the
entries; proof and audit depend on `include`. Keep identity, declare omitted
fields, and run the widening actions with the same scope, verb, cursor or cut,
interval, window and limits. Compare the proof too: keeping only the clock's
name does not keep the instant. If several entries share a full-packet action,
run it once. A change in hidden content must also invalidate the cursor.
The SQLite composition of `EmbeddedKernel` pins one read view for a whole
memory operation: abouts, membership check, graph, bodies and sources. Clones
of its ports share that view; it does not enter a cursor and is not persisted
between calls. Errors opening it propagate, without falling back to live
ports. Compositions over separate stores need a provider that can guarantee
that joint state; a distributed transaction is not promised. Test real
concurrent changes, missing data and cancellation, as well as byte
equivalence. The consistency statement must give the scope of one call, not of
the whole conversation.

Widening is a new read, not a snapshot promise. Measure both selective
navigation and widening of every result.

When changing selection by `refs`, check that it applies before the entry or
window limit, keeping scope, labels and clocks. The cursor resolves against the
admitted history; the focus does not cut the dependency sources. Keep the
question's cut even when the chosen memory is older. Measure equal behaviour
without `refs`, recovery of proof exactly at the cut and exclusion of later
proof. Tell apart refs that do not match, history outside the packet and proof
still to be paged; none of them proves global absence. Update the canonical and
the vendored protobuf, both adapters and the runnable example together with the
MCP contract.

The no-repetition rule is about the load in context. A source file and its
generated assets are not two manuals the agent must read. The common rules that
already arrive in `initialize` need not be copied whole into every description
either. Check what the host presents before blaming the server for a
repetition added outside native MCP.

The full contract's fields declare types, accepted values, bounds and
defaults; `tools/list` publishes only types, enums and `required` (see
[published law](#published-law-cross-field-rules-and-annotations)). Argument
descriptions, which `kmp_guide` serves in each card's `parameters`, explain the
decisions those fields do not express: which clock to select, what a filter
hides, how to continue and what counts as proof. Examples and the guide cards
can develop those explanations. When shortening descriptions, compare the rest
of the schema before and after too: a text reduction should not hide the loss
of a field or a condition. That comparison is review evidence, not a new
editorial gate.

The `initialize` instructions contain only decisions common to every verb:
activation, initial recovery, temporal versus semantic choice, continuity,
identifiers, proof and the trust boundary. Specific use belongs to the tool's
guide card: the `ask` card explains its retry and UNKNOWN; the `time` card
explains the inclusive bound of a forward interval; the `write` card explains
the search summary and evidence. A tool description is one sentence. Some hosts
put `initialize` in front of every description: measuring that representation
avoids multiplying a common manual. Do not couple this split to tests of
sentences or of text order.

## 3. Regenerate with the matching engine

From the repository root, use a separate target if there is a frozen binary
kept for measurements. Do not overwrite it, and do not sync development
examples into the personal store.

When switching between worktrees, prefer one development target per checkout.
A shared target may keep another checkout's assets even though the binary
starts. If that happens, clean only the affected crate and rebuild its
dependents before generating or reviewing. Check the visible result and record
the hash of the binary used; do not assume a capture represents the code
because of its path.

```bash
export CARGO_TARGET_DIR="$PWD/target/guide-development"
cargo build --locked -p kmp-mcp
cargo run --locked --quiet -p kmp-release -- guide assets write \
  --binary "$CARGO_TARGET_DIR/debug/kmp-mcp"
```

Generation uses the binary's live contract and rebuilds the two guide abouts.
Review source and outputs together. If the public story changes:

```bash
cargo run --locked --quiet -p kmp-release -- readme sync
```

If what MCP announces or returns changes on purpose, regenerate its fixtures
and review the whole diff. This step does not apply to a mere editorial
reorganization:

```bash
KMP_BLESS_TOOL_SURFACE=1 cargo test --locked -p kmp-mcp --test tool_surface_parity
```

## 4. Review the use and check the behaviour

If temporal widening with `include.dependencies` changes, review together the
four moves of `kmp_time` (`goto`, `near`, `forward` and `rewind`),
`TemporalInclude` and the canonical and vendored protobuf responses, the gRPC
adapters, group selection and the MCP projection and paging. `entries` keeps
the historical selection; complete supporting records go in `proof.entries`
and the members in `proof.groups`. Do not count those supports as new matches,
nor read structural coverage as a verified answer.

The checks must use related old and recent sources and exercise an absent
clock, a future or expired relation, a dimensional predicate, the widening
limit, field reduction and reconstruction of every page. Changing only one
supporting record must invalidate an earlier continuation. Check the real gRPC
service with test repositories and, separately, the MCP transmission and
projection of its fields; a simulated server alone does not verify the
selection. Update `verbs/time.md`, the example
`plugins/kmp/guide/examples/alias-ownership.md` and its editorial evidence,
regenerate the assets and run that example against the new binary. This
guidance documents how to maintain the surface; it adds no editorial gate to
CI.

When adding a new place that carries memory text, also review the composer's
typed paths: passages, reference definitions and span sources. `proof.entries`
takes part in all three; member identifiers in `proof.groups` stay literal.
Test exact widening, paging and group omission without reintroducing a source
through the passage table. Keep metadata and actions outside text
substitution.

For a guide or routing change, run:

```bash
cargo test --locked -p kmp-release --test guide_editorial_contract \
  --test plugin_package_contract
cargo test --locked -p kmp-mcp --test guide_sync --test guide_topics
cargo test --locked -p kmp-adapter-embedded --test guide_bundle
python3 scripts/ci/kmp-capability-contract.py
```

Wording, length and organization of the guide are reviewed informatively. Do
not create sentence validators, arbitrary size limits or simulations that only
compare traces already written into fixtures. Nor is a domain or dedicated
application layers needed to enforce this procedure. Native tests check the
operation KMP runs; the document explains its use.

Also run the tests of the affected behaviour. For Rust, use fmt and clippy as
in [Testing](testing.md). Choose the checks by the change; do not update
expectations about evidence, clocks or selection without justifying it.

Replay an affected lesson in an isolated store:

```bash
guide_review_dir="tmp/guide-review-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$guide_review_dir"
python3 scripts/guide_examples/replay.py \
  --binary "$CARGO_TARGET_DIR/debug/kmp-mcp" \
  --lesson alias-ownership --guide-mode markdown \
  --trace "$guide_review_dir/native.jsonl" \
  --result "$guide_review_dir/result.json"
```

Use the new case when there is one. Check that the entry is read once, that
the needed verb is reached, that pages are continued and that the guide is
reused while the same asset stays in context. Do not reuse answers from
changing memory as if they were static documentation. After compaction, a
change of store or of asset, revalidate what is still available.

For changes in representation, navigation or the human guide, add
`--hold-view` to the replay and review the context in ChronoLoom: about, clock,
interval, memories, labels, direction of relations and sources. Save the
observation and close the temporary viewer. Do not keep capability URLs with
their credentials.

## 5. Measure what the agent actually sees

Record these representations separately:

- the native MCP response to `initialize`;
- the native MCP response to `tools/list`, with descriptions and schemas;
- the catalogue or metadata the host exposes;
- the skill and the initial entry;
- guide lookups and the results of the working journey.

Also compare the callable signature the agent sees with the native schema. An
object valid in `tools/list` may appear as `unknown` if the host favours a
conditional composition and drops its sibling properties. Review that case
with the observed representation, keep the contract's constraints, and record
the check after loading the change. A local projection does not prove that an
open session has renewed its tools.

Count bytes and tokens with the tokenizer's name, version and configuration.
Keep text or traces and hashes. Compare first use, first lookup of a verb,
repetition of that verb and a whole journey with equivalent coverage. If
detail is removed from the initial context, count the later read of that detail
too. Do not add two alternative representations as if both were injected, nor
present these counts as billing or a money saving.

For write errors, assign a code and a path where the field is validated. Do
not deduce them from the error text. Keep the category of a backend failure; a
repair instruction must not turn it into a caller error. If a read is
proposed, include the full name and arguments and test that it can run. If
evidence is missing, point at the field without fabricating a payload. The
writer's kind vocabulary lives in `contract/writer_memory_kinds.rs`: the
schema, validation and `allowed_values` consume it directly. When extending it,
teach when the kind applies and verify refusal, source-based choice and
commit; never add an automatic replacement deduced from the invalid name. Also
review the schema errors that come before the planner and the packet indexes.
Tests must walk refusal, read and correction, and commit, verifying that the
refused packet produced no partial writes.

When the change depends on how an LLM learns or decides, test it separately
with new sources and questions hidden from the writer, within the authorized
budget and pause state. Native validation does not resume a measurement with
models by itself.

## 6. Leave a close that lets the work continue

The delivery must name the commit, the sources changed, the assets
regenerated, the tests run, the cost before and after, and the limits. Say
whether it is only on a branch, merged or already installed. An open session
may keep exposing earlier metadata: do not confuse the delivered code with the
surface that session uses.

Keep defects and pending actions in their issue. A Markdown increment does not
complete a review of the whole surface. Bugs follow their PR and checks;
capability changes follow the agreed iteration flow. Publishing and
installing follow [Releasing](releasing.md).

Update this procedure when an owner, an asset, a loading rule or a command
changes. `AGENTS.md`, the development index and the guide README must keep
pointing here, so a person and an agent find the same maintenance contract.

Writer receipts use the accepted command event and idempotency index. Keep the
compact acknowledgment, native/gRPC ingest mapping and Inspect audit detail in
step. Verify actual returned actions across restart, subsequent memory changes
and export/import; previews and refusals must create neither a receipt nor
memory. Coverage describes submitted declarations, never completeness against
sources the writer did not submit. Do not add graph memories or another CI rule
for receipts.

# Contract notes

How specific parts of the surface work today, and what to keep together when
changing them.

## Selection fingerprints and transport changes

In Wake, `scope.selection` identifies the proof and the sections subject to the
clock and to the interval or instant declared in `proof`. `scope.context`
identifies the about's context, whose time is `unbounded`; it may describe
memories outside that historical selection. `scope.dimensions` declares the
filters applied to both groups, with their values and predicates. The
catalogue keeps the labels of the entries that pass: filtering by one label
does not remove their other labels. Keep this signal inside the budget and
across the typed transport. Verify a window with proof, another without proof
but with context, and dimensional filters; an about that does not exist is a
different case. `wake.objective` comes from the requester; it is not retrieved
evidence. Teach these differences in the verb and in a lesson with sources,
also visible in ChronoLoom.

Trace and Relate compute their fingerprint over the complete selection before
cutting the page. When adding a field to those results, include it in the
fingerprint if it changes observable content, evidence or scope. The order of
metadata maps has no meaning: its representation for the hash must be
deterministic. Do not bind the hash only to the page that fits, nor send the
whole selection to the client to validate it. MCP binds the fingerprint to the
query and also counts the continuation actions inside the response budget.

Transport fields are edited in `api/proto` and synced with the copy in
`crates/kmp-proto/proto`. MCP and the kernel must share that contract; a
backend without a fingerprint is refused explicitly. Check continuation, a
change on a later page and the restart action in the affected transports. This
explanation is informative maintenance; it adds no CI check.

## Ask and Wake continuations

`kmp-proto-mapping/src/v1beta1/recall_projection.rs` counts the action before
the expansion. `recall_actions.rs` (same directory) builds the call and its
sufficient budget. When adding an argument to Ask or Wake, update its
conversion to native arguments, the cursor identity and the test that runs the
received call. Keep the temporal selection, the dimensions and `asked_as`;
leave out an unlimited `max_entries` instead of sending a zero that MCP
refuses.

The MCP response uses `projection.next_action: {tool, arguments}`. The proto
carries `RecallCall` in `next_call`, with JSON arguments so integers stay
exact; the old prose field is no longer emitted and has no compatibility
adapter. Typed errors carry `restart`, and MCP exposes it as
`feedback[].action`. Keep both copies of the proto and check equivalent
execution across the transports.

## Continuation handles

A page does not repeat the request. The projector (gRPC, `next_call`) still
emits the complete call; the MCP server stores it and returns
`{"continuation":"read_<32 hex>"}`. Only continuations are shortened: an action
with `page.cursor` (Wake, Ask, Trace, Relate, Inspect, the temporal moves) or a
write that resumes its review (`review_token`). A restart without a cursor
(`core_text_shortened`, `feedback[].action`) and temporal navigation repeat
their call, because the agent has to read them to choose.

| Piece | Where |
| --- | --- |
| Shorten and resolve | `serving/read_continuations.rs` (`shorten_read_actions`, `resolve_read_arguments`) |
| Store | table `open_continuations` in `continuations.sqlite3` beside the store (`continuation/adapters/sqlite_continuation_store.rs`); in memory if the server has no path. A 0.27.0 `agent-users.sqlite3` is no longer opened |
| Input schema | `contract/registry.rs`: beside `continuation`, any declared argument may be resent, and the rule `continuation-alone` refuses a value the retained call does not have |

The handle is 128 random bits: it names a call and grants nothing. HTTP
resolves before authorizing and authorizes the resolved call. The stored call
keeps its cursor, so a change of selection is still refused with `conflict`
and a full restart in `feedback[].action`. Any argument of the retained call
may be sent beside the handle with the value the call already has, and is
ignored: the answer is the continuation's own. A different value is refused
with `invalid_argument` under `continuation-alone`, naming the argument
(`feedback[].field`, code `CONTINUATION_ARGUMENT_DIFFERS`); Wake and Ask also
accept `page.repeat_core`. To change the page size or the budget, send the
original arguments with `page.cursor` (the stateless path still works).
Retention: 24 h, 256 calls per directory, 32 KiB per call
(`sqlite_continuation_store.rs`). A handle that expired or belongs to another
store returns `CONTINUATION_UNAVAILABLE`. The `tool_surface_parity` fixtures
redact the handle; parity with the real kernel (`kmp-tests-kernel`) compares
with the continuation arguments masked, because the direct backend has no
server state.

An action without a cursor after `core_text_shortened` restores the core with a
new read: discard the earlier partial reconstruction. Ordinary continuations
add only expansions. Test the whole journey against a wide read of the same
store, restoring from 512 bytes, limits and errors, without parsing the
diagnostic text. Measure arguments plus responses of the whole journey,
restarts included. Recovery proposes the sufficient size plus 10,000 bytes for
expansion; offering only the minimum can multiply calls. In parity tests,
isolate `XDG_DATA_HOME` so the local store does not load a personal lexical
table that the test server lacks; compare the same retrieval configuration on
both paths. The native check does not measure LLM learning or host billing.
These instructions are informative documentation.

`projection.sections.*.remaining` counts the eligible queue after the page,
without the repeated core or expansions of earlier pages. Compute it over that
queue, not by subtracting what was returned from a total, which counts again
what was already read.

Section counters (`kmp.recall.projection.v3`). A counter at zero is omitted,
and so is a section with none. Each cause of omission stays distinguishable:

| Counter | Page | What it says | Before v3 |
| --- | --- | --- | --- |
| `core` | the one carrying the core | core items in the section | same |
| `excluded_by_detail` | the one carrying the core | excluded by `budget.detail` in the section | `total - eligible` |
| `returned_on_page` | all | new expansion on this page | same |
| `remaining` | all | eligible expansion still pending | same |
| `eligible` | — | `core` + every `returned_on_page` + the last `remaining` | retired |
| `total` | — | `eligible + excluded_by_detail` | retired |

`page.offset` (earlier pages), `selection_omitted` (entry cap) and
`core_text_shortened` stay in `projection`. Proto: `RecallProjectionSection`
reserves 4/5 (`eligible`, `total`) and adds `excluded_by_detail = 7`. The
temporal moves and Inspect apply the same rule to `page.sections`
(`returned_on_page`, `remaining`; no `total`), and Inspect retires
`page.omitted`, which repeated `remaining`. Serialize it in
`RecallProjectionSection` too, and in both directions of the mapping; metadata
is part of the budget before text is selected. Validate first, middle and last
pages, a shortened core and exclusions by detail or capacity. Zero does not
express semantic sufficiency. The restart of a shortened core takes priority
over the notice to continue pages; also keep the exclusion indicators and the
executable action. The byte reserve uses the longest notice among those
actually emitted; avoid a planning paragraph longer than all of them, which
can displace evidence and add pages without telling the agent more.

## Published law, cross-field rules and annotations

`tools/list` publishes what an agent needs for a valid first call: types,
enums, `required` (with the single `if`/`else` that says a continuation needs
nothing else) and the `pattern` of `memories[].id`, which the agent invents.
The rest of the law is stated once: the server instructions say that every
object refuses the keys its schema does not list (the validator closes by
default every `type: object` node without `additionalProperties`); each rule
across arguments is documented in its topic's card and named with its fix in
the refusal; bounds that only repeat a check stay in the full contract. Do not
use `$ref` or `$defs`. When adding a conditional, tag it and write the rule;
when adding a bound that prevents a wrong first call, add it to
`PUBLISHED_BOUNDS`.

A tool's description is one sentence: what it is for and when to choose it
over its neighbours. Everything else goes to its card (at most 2,000 tokens;
split the topic if it does not fit). The annotations are true:
`readOnlyHint: true` only if the call does not change memory (what it may
leave is the log, quality telemetry, a continuation handle when there are more
pages and, with Jev, cached verdicts); `openWorldHint` is false except on the
tools that the session store's Jev opt-in makes send text to TypeSafe
(`KernelMcpToolBackend::open_world_tools`, applied in `tools/list`);
`destructiveHint` and `idempotentHint` are omitted when they equal their MCP
default. `kmp_inspect` is read-only: inspecting a guide node the store lacks
answers `GUIDE_UNAVAILABLE` with the repair instead of seeding the guide.

## Guide topics and errors

`kmp_guide` is a stateless lookup: without `topic` it returns the index; with
`topic`, that topic's card and the prose of the arguments it owns. The cards
are compiled into the engine from `crates/kmp-mcp/src/guide/cards/`, an exact
copy of `plugins/kmp/guide/cards/` (the editorial source of the stored guide;
`tests/guide_topics.rs` checks that they match). It neither reads nor seeds the
store and answers the same way on every install. There is no identity, context
or delivery record.

When adding a topic, update `guide_topic.rs`, the card in both places, its
entry in `editorial.json` and its link to the verb; regenerate the assets.
`initialize` and the skills name `kmp_guide {topic}`; the Markdown is the
alternative. Do not load both.

The generated entry shows the verb groups and two maps of examples. The full
topics and lessons stay in KMP. Look up the verb before its first use if it is
not in context, and open the example needed; do not load every body to
complete the index. Having looked a topic up earlier does not prove
understanding, nor that it is still in the current context.

Refusal help is decided by tool name, code and feedback path, never by parsing
the message: one line `Help: kmp_guide {"topic":…}` with the topic that owns
the first refused argument. Keep the direct repair and restart actions, the
backend codes and the absence of writes in a refusal. A guide lookup neither
retries nor syncs automatically. Infrastructure failures and guide read
failures do not create a help loop. UNKNOWN is still a valid semantic answer.

Test that the topic pointed at exists and explains the refused tool. Measure
the entry and the later reads: moving a list into a card does not remove its
cost when that card is looked up.

## Write-time context

When changing which relations require review, keep the native predicate
`memory/write_neighborhood.rs`, the compiler, `needs_review` and the
recommendation consistent. `strict:false` and local links do not get around
the condition. SQLite allows several sessions over the same store. Comparing
the revisions before and after reading each about avoids accepting a mixed
neighbourhood. The SQLite commit checks every about read again, foreign
endpoints included, and stores event, receipt and projection in one
transaction. The service's local lock only orders its own instance; it does not
show exclusion between processes. Separate event and projection adapters keep
that local limit: this guarantee across engines belongs to the SQLite backend.
Keep the idempotency check before requiring new context for an operation
already accepted. No transaction stays open while the agent reviews the
answer. Direct maintenance or import operations on ports require a store with
no active writers; they are not coordinated writes of the service.

The token includes the logical proposal and the complete selected material,
omissions included. Continuations use the existing directory: they resolve
before authorizing, keep the verb and the identity of the batch, survive a
reconnection within their expiry and never grant permissions. If the batch
cannot be retained, return the complete action. An acknowledgment of delivery
is not proof of understanding nor of semantic sufficiency.

Verify changed context, an accepted retry, the same batch, foreign scopes, an
old constraint, contradictions, long text with a final negation and the costs
of the complete MCP result. Show link indexes as packet indexes, not as refs.
Keep the omissions and the native widening. Update the card, the verb, the
entry, the session instructions and the examples: each affected example must
show preparation and resumption explicitly; the replay must not confirm them
behind the scenes. Do not add editorial CI gates for these recommendations.

## Default writer

`serving/default_actor.rs` fills `actor` on `kmp_write_memory`, `kmp_relabel`,
`kmp_condense` and `kmp_curate` apply when it is missing: the `clientInfo.name`
of `initialize`, or `mcp-client`. It applies before planning, so the
idempotency digest and the retained continuation carry it. An explicit actor
wins.

## Inference examples

The files in `api/examples/inference-prompts` are adaptable examples, not a
second MCP contract. A consumer builds its request from the `inputSchema` of
the current `tools/list`. Do not require equality of the whole schema between
a static example and the engine: a new optional capability does not by itself
invalidate an example that does not use it. Review the behaviour illustrated
when changing it; keep the tests of the native contract and of the real
vocabulary.

## Write result signals

The default observation is resolved in `kmp-application/memory`, using the
same instant ingestion assigns. The semantic compiler turns on
`default_observation_to_ingestion` in the canonical command; the gRPC adapter
carries that policy. Do not compute the date in MCP before the digest: the same
retry would have different content. The digest uses the unresolved command,
and the receipt stores the resolved memory and provenance. Canonical ingestion
without that policy keeps absent times absent. The already ingested
coordinates of a summary update are kept, an unknown observation included.

When changing the defaults, review the root and record schemas, inheritance and
explicit null, the normal, batch and summary compilers, the protobuf in both
copies, the mapping and the application. Keep `clock_defaults` (plan) apart
from `clocks` (accepted result), and check preview, atomic refusal, retry,
restart and import. Update the write and time cards, the extended verbs and the
four-clocks example; explain the human contract in the `advanced:clocks` node.
This list is maintenance guidance, not an editorial CI validator.

The committed/replayed decision comes from `update_context` at the point that
checks idempotency. Do not infer it from a host retry or from a persistent
agent identity. The clock summary comes from the canonical memory of the
accepted command; store that summary in its receipt and read it back on a
replay, without replacing it with the clocks of a new translation. Count
memories once, not dimensional memberships. Convert the internal instant
through the existing temporal mapping before showing it to the agent.

A preview stored no clocks, and a transport error does not show a refusal
without persistence. Keep validated, rejected and unconfirmed distinguishable.
Verify a retry after restart, a later write and import, and the embedded and
gRPC transports. Measure the cost of the complete receipt; keep access to the
detail without copying all the evidence again or creating telemetry memories.

`relations` shows `{from, rel, to}` triples derived from the plan's canonical
relations. Local endpoints are shortened to `@id`, resolvable through
`local_refs`; external ones keep the ref. Do not rebuild them from the
diagnostic, nor deduplicate by relation name: two links of the same type can
have different targets. Check preview, commit and replay against the real
receipt.

When changing a relation, describe its source and target roles in
`KnownMemoryRelationType::writer_spec`, review the write card, the verb and the
affected examples, and regenerate the assets. Include a counter-example of
direction or scope when it avoids ambiguity. `supersedes` marks the whole
target SUPERSEDED; `corrects` does not change its lifecycle state. Do not
promise that acceptance or the rich category verifies meaning. This editorial
review is informative: do not add a sentence or size gate to CI.

## Output schemas on request

`tools/list` does not announce `outputSchema` by default; MCP makes it
optional, and `structuredContent` is still returned. The schemas stay in code
(`contract/`) as the contract of the responses, and tests validate against
`tools_list_result()` or `kmp_mcp_tools_list_result_with_output_schemas`, never
against the default catalogue. `KMP_MCP_OUTPUT_SCHEMAS=on` announces them for a
host that validates output; it reads the on/off set every switch reads (`on`,
`off`, `true`, `false`, `1`, `0`, `yes`, `no`, any case), unset or blank means
off, and any other value stops startup. `serving/output_schema_projection.rs`
removes them at the end, after any projection that widens them; a projection
that patches `outputSchema` must check first that it exists.
`fixtures/contract/tools_list_with_output_schemas.json` pins the catalogue with
schemas; `tools_list.json` and `tools_list_with_apps.json` pin the announced
one.

## Optional shared-passage projection

`KMP_MCP_PASSAGES=shared` turns on the representation at the host; it does not
change the read arguments. `serving/passage_projection.rs` adapts
`initialize`, `tools/list` and the read responses after their budgets and
recommendations. The library `kmp-proto-mapping::context_projection` keeps the
reversible codec and the composition by whole groups. Keep tables local to
each response, distinct sources even when they share text, and read actions
intact. See [contract and limits](context-projection.md).

Check both configurations and their announced schemas. The guide files are
generated from the normal contract and also teach the optional mode. The
saving in MCP traffic and the saving in composed context are different
measurements; add the extra instruction and schema of the active mode. Do not
put a tokenizer policy into the kernel, nor infer source limits to merge
overlapping quotes.

Composition is offered through `kmp-mcp context project|expand [FILE|-]`,
without opening a store or running the declared reads. When changing the
context contract, update its version, decoder, CLI and runnable example;
refuse unsupported versions and keep the frozen binary to read earlier
artifacts. Expand each native table inside its own page before gathering
groups. Quote limits come from the host: verify ref, hash, UTF-8 and whole
text, without attributing truth to that match. The reader is given ordered
fragments it concatenates with no separators; it need not compute offsets.
Check a source left out by budget, independent testimonies and exact recovery
of constraints, conflicts and UNKNOWN. Measure bindings, tables and manifests
too; the saving on the text body alone does not represent the whole context.

## Trace with a temporal selection

`as_of`, `interval` or a `to` list turn on the bounded walk. The canonical
coordinates of `contains_entry` and the edges are read in the same
transaction; do not use the node's original payload after a relabel. The
shared temporal predicate lives in domain; the adapters apply the budget
before reading. Indexes by type avoid walking the whole fan-out to find
coordinates. Check real SQLite queries, not only how many rows the result
returns.

When changing this surface, keep and include in the fingerprint the clock, the
resolved cut, the resolution state, the coordinate work and the indexes of
relations without a clock. Test an inclusive cut, a half-open interval, a ref
without a clock, a budget exhausted during admission, a later edge between old
endpoints and a concurrent change of coordinates. The page continues the
selected result; it does not continue the search nor promise a snapshot
between calls. Keep the warning that a route does not establish truth or
completeness of proof. These notes are informative, with no editorial gate.

## Relations' own clocks

The semantic declaration is dated at ingest, once per accepted command.
`MemoryRelationClocks` separates those clocks from dimensional membership;
`connect_to` requires no more fields. In semantic batches, observation comes
from the member's declaration: inherited from the packet, its own override, or
exact ingestion for null. The compiler fixes that provenance before joining
plans; do not infer it from an endpoint at ingest. Occurrence and validity are
kept only when they are declared explicitly for that relation. Canonical
restore keeps historical clocks; replay uses the receipt. Do not date old edges
during their projection. A new UPSERT represents the current stored
declaration; this does not add historical versions of edges.

Keep together the canonical and vendored protobuf, the ingest mapper, the
payload projection, the direct and compact MCP recall responses,
`VisualRelation` and ChronoLoom (HTTP and MCP App). Semantic relations show
`clocks` without needing a dimension; structural links keep their coordinate.
Do not duplicate the clocks in both groups of a semantic relation. Two explicit
incompatible values are refused. `clocks.relations` in the receipt counts link
coverage; the entry counters keep their meaning.

Verify old endpoints with a late declaration, an inclusive cut and an
exclusive upper end, omission, record overrides, restore and replay after
restart. Declaration clocks do not guarantee truth or completeness. A visual
projection selects entries: its links show the dates, but they do not replace
a temporal proof read. Update the guide, the cards and the contract fixture in
the same change, with no editorial gate.

## Evidence associations and their clocks

`memory.evidence[].time` keeps the source's instant. `support_clocks` dates the
declaration of its associations: its own observation, the packet's, or
ingestion when missing, and the ingestion KMP accepted. The semantic batch
keeps the member's effective observation here too, including a null that
clears the root. With `default_observation_to_ingestion`, an explicit clocks
object with no observation and no restored ingestion gets the ingestion
instant; an absent object keeps the packet's canonical inheritance. See the
[design and controls](../archive/development/batch-proof-observation.md). Do
not copy dates from the endpoints nor re-time the source. When changing this
contract, review the canonical DTO, both protobufs, the ingest translator, the
event projection and the evidence mappers in recall, temporal and Inspect.
Temporal admission must filter candidates and supports, not just hide the edge
in the response. Test an old source with a later association, inclusive and
exclusive cuts, replay and restore. No new mandatory fields for the writer, and
no invented clocks when re-projecting old events.

## Host adviser

The `kmp-expert` skill uses a host subagent with a limited responsibility:
explaining the protocol. Its helper gathers the `guide:kmp-agent` entries of
the canonical asset, every example included, and keeps a copy by hash in the
task cache. It adds no LLM to the kernel and no other catalogue of examples.
When changing the guide, regenerate the asset; the hash changes and the next
preparation uses the new content. The manifest records refs, hashes and lines,
to check that everything was delivered without truncation. That delivery check
does not show learning, nor avoid the cost of loading a new context.

The adviser has its own context, which the host keeps. After compaction it
reloads the complete guide. Compare the asset's revision with the store's
installed guide before consulting. The working agent keeps the decision about
the sources and runs the authorized proposals. Verify the flow with real
subagent consultations and count both participants. This evaluation is
informative, with no new editorial gates.

## Selected-node framing in ChronoLoom

The app-only `kmp_view_read_nodes` uses the same `ReadNodes` operation as HTTP
`/api/nodes`. Keep the protocol copies, request validation, the viewer's HTTP
view and the MCP JSON mapper, the app negotiation gate and both browser
transports aligned. Model-only tool lists do not gain this data tool. A node
header with incomplete coordinates must not be used to frame an exact focus;
omitted refs are not missing refs. Framing reads all named refs together
instead of combining cached scene coordinates with new inspections. Foci
larger than 64 refs send the returned `snapshot` as `expect_snapshot` in each
following batch; they share a 32,768-edge ceiling and reject any intervening
commit, including body-only writes. A batch also stops with `node_budget` after
4,096 distinct nodes, counting requested refs and the coordinate scopes reached
through them. Observer identities include a random incarnation, so a server
restart cannot reuse a prior identity. No snapshot is retained between calls.
Check late responses after a changed focus, about or clock, and keep the old
window on a conflict or an incomplete batch.

The operation reuses the bounded snapshot adjacency port and loads no
canonical bodies or proof sources. It does not replace bounded Trace proof
materialization, change historical admission, or promise a snapshot across the
later projection request. Validate headers and complete coordinates against
individual Inspect on one quiescent fixture before measuring, keep empty,
missing and foreign cases and real concurrent snapshot tests, and report
complete framing journeys separately from storage latency. See
`crates/kmp-viewer/examples/node_batch_benchmark.rs` and
`scripts/performance/node_batch_framing.py`.

## Optional consolidated views

The embedded `kmp-mcp consolidation sources|write|read|project` CLI is a
separate optional view contract, not another MCP verb. Keep source stamps,
compact acceptance receipts, immutable expansion, stale handling and explicit
clock restrictions aligned with `advanced:consolidation` in both generated
guide abouts. Verify the real CLI through restart, source mutation, invalid
requests and an exact audit expansion. Measure preparation and write cost as
well as the selected context; keep writer mistakes and budget failures when
comparing readers. Semantic fidelity is external to deterministic quote and
dependency validation. See the [contract](incremental-consolidation.md) and the
[development evidence](../archive/development/incremental-consolidation-evaluation.md).
