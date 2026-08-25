# Hugging Face Publication Assets

This folder contains draft publication assets for the kernel tool-operator
model and trajectory dataset.

These files are templates, not final public cards. Fill them only after the
P1.11 release gate is complete.

| File | Purpose |
| --- | --- |
| [kernel-tool-operator-small-model-card-template.md](kernel-tool-operator-small-model-card-template.md) | Draft model card for `underpass-ai/kernel-tool-operator-small` |
| [kernel-operator-trajectories-dataset-card-template.md](kernel-operator-trajectories-dataset-card-template.md) | Draft dataset card for `underpass-ai/kernel-operator-trajectories` |
| [operator-release-eval-summary-template.md](operator-release-eval-summary-template.md) | Public evaluation summary template for a specific release |
| [repository-visibility-checklist.md](repository-visibility-checklist.md) | GitHub visibility checklist for the model release |

Publication rules:

- do not publish model or dataset repos until offline eval and live MCP replay
  are clean;
- keep raw refs out of model-facing files;
- publish private on Hugging Face first;
- verify download and replay from published artifacts;
- only then switch the Hugging Face repos to public and update the GitHub
  README.
