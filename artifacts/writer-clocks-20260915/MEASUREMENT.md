# Compact response measurement

On 2026-09-15, the existing native #683 regression
`neighborhood_clocks_reach_the_writer_as_rfc3339` serialized its
`neighborhood` object to **1,758 bytes** before `stored_abouts` was added and
to **1,806 bytes** on the final candidate (**+48 bytes**). The builder uses
2,048 bytes as its selection target, but it can retain one oversized item; it
is not a public response-size guarantee and this measurement does not add a
test gate.

Command:

```bash
CARGO_TARGET_DIR=/home/gx10a/Documents/ai/kmp-540-consolidation/target \
CARGO_BUILD_JOBS=4 TMPDIR="$PWD/tmp" \
cargo test --locked -p kmp-mcp --test write_neighborhood_selection \
  neighborhood_clocks_reach_the_writer_as_rfc3339 -- --nocapture
```

This measures the compact neighborhood serialization for the stored/proposed
clock fixture. It is neither an end-to-end latency benchmark nor a measure of
host tokenization or writer comprehension.
