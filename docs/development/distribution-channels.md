# Distribution channels

Where people can find KMP, what each channel reads from this repository, how
it is updated, and what still has to be done by hand. The release flow in
[Releasing](releasing.md) publishes the first group; the second group is a
listing someone submits once and this repository keeps true afterwards.

The one-line description every channel opens with is the workspace
`description` in `Cargo.toml`. `scripts/release.sh preflight` fails when a
manifest below drifts from it (the `tagline sources` readiness check).

## Published by the release

| Channel | Reads | Updated by | Check |
|:--|:--|:--|:--|
| GitHub Releases | the tagged tree and its 22 checksummed assets | `release.yml` promote on the tag | `scripts/release/storefronts.sh` |
| `marketplace` branch (Codex and Claude Code plugin install) | `.claude-plugin/marketplace.json`, `.agents/plugins/marketplace.json`, `plugins/kmp/` | `release.sh publish`, after every asset is public | same |
| crates.io `kmp-mcp` | `crates/kmp-mcp/Cargo.toml`, `crates/kmp-mcp/README.md` (synchronized overview block) | `publish-distribution.yml` publish-crates; resume with `resume_crates` | same |
| MCP Registry `io.github.underpass-ai/kmp` | `server.json`, `distribution/mcpb/manifest.json`, the crate README's `mcp-name` marker | `mcp-registry.yml` on the tag while `MCP_REGISTRY_PUBLISH` is armed | same |
| MCPB (Claude Desktop and other MCPB hosts) | `distribution/mcpb/manifest.json`, the five release engines | `release.yml` mcpb, attached to the GitHub release | the release asset list |
| GHCR image and Helm chart (enterprise topology) | `Dockerfile`, `distribution/charts/kmp/` | `publish-distribution.yml` | `helm pull`, `docker pull` |

`storefronts.yml` runs the check after every published release, once a day
and on request, and turns red when any channel lags the release.

## Listings this repository feeds but does not publish

| Channel | How it lists KMP | Status and action |
|:--|:--|:--|
| [Glama](https://glama.ai/mcp/servers/smttmz3m70) | Reads the GitHub repository; `glama.json` names the maintainer | Listed and claimed. The description it shows comes from the repository description on GitHub, which has to be set by hand (below). |
| GitHub repository description, homepage and topics | Repository settings | **By hand, once:** set the description to the workspace tagline, the homepage to the README or the docs, and keep the `mcp`, `memory`, `claude-code`, `codex` topics. Several directories scrape this field. |
| Official Claude Code plugin directory | Submission form at `claude.ai/settings/plugins/submit` or `platform.claude.com/plugins/submit`; points at this repository's marketplace | **To submit, once.** The co-located marketplace (`underpass-ai/kmp@marketplace`) keeps working either way. |
| Codex plugin marketplace | `codex plugin marketplace add underpass-ai/kmp --ref marketplace` | Self-served from the `marketplace` branch; no central submission exists today. |
| [mcp.so](https://mcp.so) | Submit form; reads the repository README | **To submit, once.** |
| [PulseMCP](https://www.pulsemcp.com) | Submit form; reads the MCP Registry and the repository | **To submit, once.** Registry listing is already there. |
| [Smithery](https://smithery.ai) | Claim the server from the repository; reads `server.json` | **To claim, once.** |
| awesome-mcp-servers lists | A pull request adding one line under memory servers | **To open, once,** with the blurb below. |
| Comparison articles ("best MCP memory servers") | Editors read the README and the examples | Send the blurb and the `examples/` link when the README's first screen is final. |
| ChatGPT apps | `chatgpt-app-submission.json` describes the app and its tools | **To submit** through the OpenAI apps flow once the stdio server is reachable from that host. The file is kept in sync with the tagline. |

## The blurb

Use the same words everywhere; they are the canonical overview block in
`plugins/kmp/README.md`.

> **KMP** is local-first memory for coding agents. It runs beside Codex,
> Claude Code and Hermes Agent as one small binary with an embedded SQLite
> store, and it remembers what was decided, what the evidence was and when it
> happened. Never the transcript. Ask "Why did we choose SQLite?" and the
> agent answers from stored evidence or says UNKNOWN. Ask "Show me the memory
> behind this decision" and ChronoLoom opens on it and lights up the proof
> path. Apache-2.0. https://github.com/underpass-ai/kmp

One-line form: *Local-first agent memory that preserves what happened, when
and why.*

## When something changes

- A new tool, host or install path: edit the canonical overview block, run
  `kmp-release readme sync`, and the next release carries it to crates.io and
  the Registry. Listings that read the README follow on their own.
- A new tagline: change `Cargo.toml` `[workspace.package] description`, then
  every manifest the readiness check names, then the GitHub repository
  description by hand.
- A channel that lags a release: `bash scripts/release/storefronts.sh X.Y.Z`
  says which one and names the job that resumes it.
