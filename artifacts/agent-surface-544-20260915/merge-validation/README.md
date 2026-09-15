# Validation against the combined main

The merge baseline is `c56b4b12` (full ID in `baseline-commit.txt`), containing
PR805, PR807 and PR808. The generated guide conflicts were resolved by
regenerating both audiences from the combined editorial source and branch
binary, with all guide probes passing.

The schema comparator repeats all 4,623 acceptance cases against the current
main catalogue, including PR805's updated output descriptions. Named property
shapes and every non-input contract field remain equal. This is a new schema
comparison; the original startup and host captures remain frozen separately.
An updated installed-host observation is still pending.

Compact native catalogue JSON: 247,194 to 247,012 UTF-8 bytes, and 51,295 to
51,236 `o200k_base` tokens. These separately serialized counts are not billed
input. Package versions, hashes and per-tool counts are in
`schema-comparison.json`.

`native-tests.log` records the combined native controls for tool surface,
neighborhood selection, writer clocks, relation-only writes, continuations,
paged reads and writer input types. Formatting and `git diff --check` pass.
