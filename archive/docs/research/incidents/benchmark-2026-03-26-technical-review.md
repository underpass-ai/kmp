# Technical review of the 2026-03-26_211733 benchmark
## What the data really supports, what it does not, and how to redesign it

## Scope

This review analyzes the benchmark artifact extracted from:

- `/mnt/data/analysis_bench/2026-03-26_211733`

Main sources inspected:

- `results.csv`
- `report.md`
- `report-full.md`
- `test.log`
- selected per-run JSON results

---

## Executive assessment

The benchmark contains **useful signal**, but it should be treated as a **directional experiment**, not as strong empirical proof.

My overall judgment is:

> The data supports the claim that **explanatory** and **mixed** contexts outperform **structural** contexts on this benchmark, but the benchmark still contains **serious methodological weaknesses** that limit any strong claim about robustness, causality, or general superiority.

The main problems are:

1. **Structural contamination** in meso/stress variants  
2. **Single observation per cell** in the factorial design  
3. **Large prompt/judge effect**, which competes with the context effect  
4. **Partial model pairing matrix**, which weakens model-level conclusions  
5. **Competing noise unexpectedly improving some metrics**, which suggests benchmark leakage or judge leniency

---

## What the data clearly supports

## 1. Context type matters a lot

Global averages by relation mix:

| Mix | Task | Restart | Reason |
|-----|------|---------|--------|
| explanatory | 0.617 | 0.458 | 0.825 |
| mixed | 0.621 | 0.375 | 0.892 |
| structural | 0.175 | 0.188 | 0.325 |

### Interpretation
This is the strongest signal in the dataset.

- `explanatory` and `mixed` are clearly much stronger than `structural`
- the effect is visible across all three reported metrics
- the effect size is large enough that it is unlikely to be just noise from one or two cells

### What this does justify
It is fair to say:

- relation mix is a major factor in benchmark performance
- explanatory/mixed context is much more useful than structural-only context in this setup

### What this does **not** justify yet
It does **not** justify saying:

- this is robust across random seeds
- this generalizes to real agent tasks
- this proves superiority over external systems

---

## 2. Prompt design has a very large effect

Global averages by prompt:

| Prompt | Task | Restart | Reason |
|--------|------|---------|--------|
| citation-agent | 0.653 | 0.403 | 0.722 |
| default | 0.535 | 0.361 | 0.660 |
| strict-judge | 0.535 | 0.396 | 0.667 |
| lenient-judge | 0.514 | 0.333 | 0.694 |
| v1-original | 0.118 | 0.208 | 0.660 |

### Interpretation
The prompt is not a small modifier. It is a major driver of outcomes.

The biggest warning sign is `v1-original`:

- `task = 0.118`
- `restart = 0.208`
- `reason = 0.660`

So the benchmark is highly sensitive to prompt framing, and some gains may come from prompt/judge behavior rather than context alone.

### Consequence
If the analysis presents improvements as “caused by context” without properly qualifying prompt effects, that is too strong.

---

## 3. Reason preservation is easier than restart detection

Global averages:

| Metric | Global mean |
|--------|-------------|
| task | 0.471 |
| restart | 0.340 |
| reason | 0.681 |

### Interpretation
The benchmark currently favors **reason preservation** more than **restart correctness**.

This suggests the system is better at producing or preserving explanatory text than at locating the correct operational restart point.

This is consistent with a context system that is already good at explanation, but still weaker on re-execution continuity.

---

## What is methodologically problematic

## 4. Structural variants are semantically contaminated at meso/stress scale

This is the most serious issue in the dataset.

From `test.log`:

### Micro structural
- `micro-ops-structural-clean`: `reason=none`
- `micro-ops-structural-competing`: `reason=none`

### Meso structural
- `meso-ops-structural-clean`: reason contains multiple `distractor branch ...` strings
- `meso-ops-structural-competing`: reason contains multiple `alternative operational response required path ...` strings

### Stress structural
The same pattern appears again at larger scale.

### Why this matters
This means that “structural” is **not consistently reason-free**.

At micro scale, structural behaves like “no reason”.  
At meso/stress scale, structural behaves more like:

> no correct reason on the main path, but many reason-like distractor strings in competing or distractor branches

That is a different benchmark condition.

### Observable consequence in the results
By mix and noise:

| Mix + Noise | Task | Restart | Reason |
|-------------|------|---------|--------|
| structural + clean | 0.200 | 0.083 | 0.008 |
| structural + competing | 0.150 | 0.292 | 0.642 |

This is the key red flag.

A “structural” benchmark should not jump from `reason = 0.008` to `reason = 0.642` just because competing branches were added, unless the benchmark is letting alternative reasons leak into the judged answer.

### Conclusion
Any analysis claiming that:

- structural context also preserves reason well
- competing variants demonstrate robustness of reason preservation
- reason survives even without explanatory context

is **not trustworthy** in the current benchmark design.

The likely reality is:

> the benchmark allows the model to surface distractor or alternative reasons, and the judge often accepts them as valid reason preservation.

---

## 5. There is only one observation per factorial cell

The benchmark contains:

- 720 total rows
- 36 variants
- 5 prompts
- 4 model pairings

This looks like a complete factorial design, but it appears to have **one run per condition**.

### Why this matters
This means:

- no seed variance per cell
- no repeated trials
- no estimate of within-condition instability
- no confidence in robustness under repeated execution

### Important clarification about the report’s confidence intervals
`report-full.md` reports Wilson confidence intervals for success rates.

Those intervals describe uncertainty in the observed Bernoulli rate **within this fixed sample**, but they do **not** solve the deeper problem that:

- the sample contains only one observation per experimental condition
- there is no repeated-run variability estimate
- model stochasticity and prompt sensitivity are not separately measured

So the confidence intervals are not useless, but they should not be mistaken for evidence of experimental robustness.

---

## 6. The model comparison is not fully clean

The benchmark uses these model pairings:

- `qwen3-8b -> opus-4.6`
- `qwen3-8b -> gpt-5.4`
- `gpt-5.4 -> opus-4.6`
- `opus-4.6 -> gpt-5.4`

### Problems
1. The matrix is **partial**, not fully crossed  
   - there is no `gpt-5.4 -> gpt-5.4`
   - there is no `opus-4.6 -> opus-4.6`

2. `qwen3-8b` appears twice as often as an agent as the other two models  
   - qwen: 360 rows
   - gpt: 180 rows
   - opus: 180 rows

3. Judge effects are large, so agent-only conclusions are hard to isolate

### Consequence
The data supports statements about **pair behavior** better than it supports statements about intrinsic model quality.

So a claim like:

- “model X is better than model Y”

would be too strong unless the evaluation is restructured.

A safer claim is:

- “the benchmark is highly sensitive to the agent/judge pairing”

---

## 7. The “competing” noise mode is suspicious

Global averages by noise mode:

| Noise | Task | Restart | Reason |
|-------|------|---------|--------|
| clean | 0.500 | 0.292 | 0.592 |
| competing | 0.442 | 0.389 | 0.769 |

### Why this is suspicious
Normally, competing alternatives should make the task harder.

But here:

- `restart` improves under competing
- `reason` improves dramatically under competing

That strongly suggests one or more of these:

1. the competing branch adds useful clues rather than harmful distractors
2. the judge is accepting plausible but wrong reasons
3. the benchmark does not enforce “correct main-path reason” strictly enough
4. the competing design is semantically richer, not just noisier

### Consequence
The label `competing` cannot currently be interpreted as “strictly harder” in a causal sense.

---

## What I think of the existing report

## What the report gets right
The report is useful in that it clearly surfaces:

- large performance gaps by relation mix
- strong prompt effects
- agent/judge asymmetry
- large weakness in restart
- non-trivial latency differences

As an exploratory artifact, it is valuable.

---

## What the report overstates or risks overstating

### 1. It makes the benchmark look more statistically solid than it is
The presence of:

- confidence intervals
- tables
- hypothesis graphics
- many figures

creates an impression of statistical maturity that the experimental design does not fully deserve.

The benchmark is still **single-run-per-condition**.

---

### 2. It does not adequately foreground the structural contamination problem
This is the largest validity issue in the dataset and should be a first-class caveat.

At the moment, a reader could easily look at the summary tables and conclude something false about structural reason preservation.

---

### 3. It risks implying that competing mode demonstrates robustness
Given the current data, the opposite is more plausible:

> competing mode is entangled with alternative reason leakage and/or judge permissiveness.

---

### 4. It risks over-claiming model differences
Because the model matrix is not fully crossed and pairing effects are large, the report should be much more cautious about model-level conclusions.

---

## What I would conclude from this benchmark today

I would conclude the following:

### Safe conclusions
1. **Relation mix is a strong driver of performance** in this setup.
2. **Explanatory and mixed contexts outperform structural contexts** by a large margin.
3. **Prompt design materially changes outcomes**, so prompt effects must be treated as first-class.
4. **Reason preservation is much easier than restart accuracy** in the current benchmark.
5. The benchmark is **useful for design iteration**, especially for:
   - relation modeling,
   - prompt design,
   - judge behavior,
   - and restart-focused improvements.

### Unsafe conclusions
I would **not** conclude:

1. that the results are already robust
2. that structural context genuinely preserves reason in harder settings
3. that competing mode proves noise resilience
4. that one model is intrinsically better than another in a strong general sense
5. that these results establish broad superiority over external approaches

---

## How I would redesign the benchmark

## A. Fix the structural condition
This is the highest-priority correction.

### Requirement
If a variant is labeled `structural`, then it must not contain reason-like strings in distractor or competing branches that can satisfy the judge.

### Practical rule
For structural variants:

- no main-path rationale text
- no competing-path rationale text
- no distractor rationale text
- only topology and labels needed for structure

If you want “structural + distractor reasons”, that should be a **different named condition**, not structural.

---

## B. Separate “correct reason” from “plausible reason”
Right now the benchmark appears too permissive.

### Replace one binary `reason` metric with at least two metrics:
1. `reason_correct_main_path`
2. `reason_plausible_but_wrong`

Optional third metric:
3. `reason_supported_by_context`

This would make the benchmark much more diagnostic.

---

## C. Add replication
Minimum improvement:

- at least **3 seeds per cell**

Better:

- 5 seeds per cell for the most important comparison slices

Without this, you still do not know whether the effect is stable.

---

## D. Make the model/judge matrix cleaner
Choose one of these:

### Option 1
Hold the judge fixed and compare agents.

### Option 2
Hold the agent fixed and compare judges.

### Option 3
Run a fully crossed matrix and analyze pair effects explicitly.

Right now the current design is informative, but not clean enough for strong model claims.

---

## E. Redefine noise conditions
Current `clean` vs `competing` is not reliable as a difficulty axis.

A better design would separate:

- `clean`
- `irrelevant-structural-noise`
- `plausible-distractor-reason`
- `conflicting-main-path-reason`
- `competing-restart-point`

That would tell you much more about failure mode.

---

## F. Add a restart-specific benchmark pass
Since restart is currently the weakest metric, it deserves its own benchmark design.

Suggested metrics:

- `restart_exact`
- `restart_off_by_one`
- `restart_on_competing_branch`
- `restart_missing`
- `restart_explained_correctly`

This would be much more useful than one flat boolean.

---

## Recommended interpretation policy

If this benchmark is shown publicly or used in documentation, I would recommend this framing:

> These experiments show strong directional evidence that explanatory and mixed contexts are materially better than structural-only contexts in this synthetic benchmark. However, the benchmark currently has structural contamination in some variants, large prompt/judge effects, and only one run per condition, so it should be read as an iteration artifact, not as definitive empirical proof.

That is accurate and defensible.

---

## Final conclusion

My final opinion is:

> The analysis is worthwhile and the dataset contains real signal, but the benchmark is not yet clean enough to support strong empirical claims. It is good enough to drive design decisions. It is not yet good enough to anchor broad claims of robustness or superiority.

In short:

- **yes, there is signal**
- **yes, the analysis is useful**
- **no, it is not yet methodologically clean**
- **yes, it can become strong with a relatively focused redesign**

---

## Recommended next actions

### Immediate
1. Fix structural variants
2. Split `reason` into stricter categories
3. Rewrite public interpretation text with stronger caveats

### Next
4. Add replicated runs
5. Clean model/judge design
6. Redesign noise taxonomy

### After that
7. Re-run and compare against the current dataset
8. Keep old results only as exploratory baseline, not as final evidence
