---
name: kmp-demo
description: Load and explore the isolated KMP checkout-latency example. Use when the user wants to see KMP memory before writing their own.
---

# KMP demo

Resolve the plugin root as two directories above this `SKILL.md` and run
`<plugin-root>/scripts/kmp-demo.sh`. Use the
printed isolated data directory for `incident:checkout-latency` and demonstrate
`kmp_wake`, semantic `kmp_ask` about why rollback failed, and `kmp_trace` from
the p99 symptom to the retry-budget constraint. Do not simulate unavailable
tools; request a session restart after the import instead.
