# Focused validation — acceptance audit 15 September 2026

These commands were executed on
`fc352d7d37ac0480cc79120a6288bfd446fa4c86` before the documentation-only
commit on this branch. The original terminal output is summarized here so the
reviewed scope and exit results travel with the acceptance matrix; the tests
were not repeated merely to manufacture a second run.

| Command | Exit result |
| --- | --- |
| `cargo test --locked -p kmp-mcp --test condense_candidates` | 7 passed, 0 failed |
| `cargo test --locked -p kmp-transport-grpc --test condense_parity` | 2 passed, 0 failed |
| `cargo test --locked -p kmp-adapter-embedded --lib consolidation` | 12 passed, 0 failed |
| `cargo test --locked -p kmp-mcp --test consolidation_cli` | 4 passed, 0 failed |

The branch did not change Rust code or generated public surface. The draft PR's
dev-loop remains the review gate for this documentation and evidence change.
