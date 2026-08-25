# Operator Local Artifacts

This directory documents local Operator benchmark artifact caches.

Large data should stay outside the repository tree so normal Git operations,
search, and editor indexing stay fast. Keep only this README and the local
`.gitignore` tracked here.

Current local cache:

- `../rehydration-kernel-artifacts/operator/p111-pageinfo-221-20260512/`:
  MemoryArena P1.11 221-task run, remote audit smoke, regenerated Operator
  trajectories, SFT split, and policy eval outputs. This is the clean current
  Operator baseline source.
- `../rehydration-kernel-artifacts/operator/longmemeval-valid-20260512/`:
  preserved LongMemEval Balanced60/100/MS30 smart-writer artifacts, exported
  LongMemEval Operator trajectories, mixed MemoryArena+LongMemEval SFT data,
  no-gold audit report, downloaded `longmemeval_s_cleaned.json`, and the
  500-item full-history smoke artifacts.

LongMemEval status:

- Balanced60 v6, 100-prefix v6, and MS30 smart-writer artifacts are legacy
  audited slices and can be used for internal comparison.
- `longmemeval_s_cleaned.json` plus the 500 full-history generated artifacts
  are quarantined for Operator training claims because the cleaned dataset
  contains repeated or normalized-colliding `session_id`s inside some
  questions. The adapter now fails fast on that unsupported shape.

See
[`docs/product/operator-training-data-audit-2026-05-13.md`](../../docs/product/operator-training-data-audit-2026-05-13.md)
for the current baseline/quarantine decision.
