# How to Conduct a Scientific Analysis of Experimental Data
## Correct procedure applied to agent, context, and rehydration benchmarks

## 1. Purpose of this annex

This document explains **how a scientific analysis of data must be conducted** in an experimental benchmark like the one you are building.

It does not focus on a specific implementation, but on the **correct procedure** for:

- formulating hypotheses,
- designing the experiment,
- collecting data,
- analyzing it,
- interpreting results,
- and communicating conclusions without overclaiming.

The goal is to avoid a very common mistake in technical projects:

> having many tables, charts, and metrics, but not yet having solid scientific evidence.

---

## 2. What "scientific analysis" means in this context

A scientific analysis does not consist solely of calculating means or drawing charts.

It consists of following a process where:

1. a **clear question** is posed;
2. a **falsifiable hypothesis** is defined;
3. an experiment is designed that can refute or support it;
4. as many sources of confusion as possible are controlled;
5. measurement is done with explicit criteria;
6. uncertainty is analyzed;
7. a clear distinction is made between:
   - observation,
   - interpretation,
   - and conclusion.

Put another way:

> scientific analysis does not seek to "prove that I am right"; it seeks to build evidence good enough to decide what is reasonable to believe.

---

## 3. Correct structure of the scientific procedure

## 3.1. Define the research question

The first phase consists of formulating the exact question.

### Poorly framed
- "Is my system better?"
- "Does rehydration work?"
- "Does the benchmark come out well?"

### Well framed
- "Do explanatory contexts improve `restart accuracy` compared to structural under the same prompt and the same agent/judge pair?"
- "Does the multi-resolution bundle better preserve the causal thread under small budgets?"
- "Does the `ResumeFocused` planner improve resumption point recovery compared to `ReasonPreserving` in operational continuity tasks?"

A good question must be:

- concrete,
- operational,
- measurable,
- and tied to observable variables.

---

## 3.2. Formulate hypotheses

After the question, a hypothesis is defined.

### Example
- **H1**: `explanatory` contexts produce higher `restart accuracy` than `structural` contexts.
- **H2**: `ResumeFocused` mode improves `restart accuracy` but reduces `reason preservation`.
- **H3**: The `citation-agent` prompt reduces grounding errors compared to `default`.

Each hypothesis must be:

- falsifiable,
- specific,
- and tied to a concrete comparison.

### Important
A **null hypothesis** must always exist as well.

Example:
- **H0**: There is no relevant difference between `explanatory` and `structural` in `restart accuracy`.

This is important because it prevents building an analysis oriented solely toward confirming expectations.

---

## 3.3. Define variables

All experimental research must clearly distinguish three types of variables.

### A. Independent variables
These are the ones you manipulate.

Examples in your case:
- context type (`structural`, `mixed`, `explanatory`)
- rehydration mode
- prompt type
- agent model
- judge model
- noise (`clean`, `competing`, etc.)
- scale (`micro`, `meso`, `stress`)

### B. Dependent variables
These are the ones you measure.

Examples:
- `task_success`
- `restart_accuracy`
- `reason_preservation`
- latency
- token count
- bundle stability

### C. Control variables
These are the ones you keep constant to avoid contaminating the experiment.

Examples:
- same base dataset
- same judge template
- same budget
- same seed if doing isolated comparison
- same number of nodes
- same noise structure

If you do not control these variables well, the analysis becomes uninterpretable.

---

## 3.4. Design the experiment

Here is where you decide how evidence will be collected.

## Factorial design
When there are multiple independent variables, the correct approach is usually a factorial design.

Example:

- 3 mixes
- 2 noise modes
- 3 scales
- 5 prompts
- 4 agent/judge pairs

This creates an experimental matrix.

### Key rule
Each cell of the experimental design must be:

- clearly identifiable,
- repeatable,
- comparable with the others.

### Common mistake
Having a large matrix but only **one execution per cell**.

This is useful for exploration, but not for making claims about robustness.

---

## 3.5. Define replication

Replication is one of the most important aspects of the scientific method.

### What replication means
Executing the **same experimental condition** multiple times to estimate variability.

### Why it is essential
Because in systems with generative models there is variability from:

- sampling,
- prompts,
- ordering,
- latency,
- judge differences,
- system noise.

### Practical rule
At a minimum:

- **3 repetitions per cell**

Even better:

- **5 or more** if you want to make claims about stability.

Without replication, you can only speak of:

- observed tendency,
- not robustness.

---

## 3.6. Define metrics correctly

Metrics must accurately represent the phenomenon you want to measure.

### Requirements of a good metric
- clearly definable,
- consistent,
- reproducible,
- and aligned with the question.

### Example of a bad metric
A binary metric `reason_preserved=yes/no` that accepts plausible reasons even when they are incorrect.

### Example of a better design
Separate into:

- `reason_correct_main_path`
- `reason_plausible_but_wrong`
- `reason_missing`
- `reason_contradictory`

This converts an ambiguous metric into a scientifically more useful observation.

### Important rule
If a metric does not correctly distinguish between:
- correct,
- incorrect,
- plausible,
- partially correct,

then the analysis will be weak even if the numbers look clean.

---

## 3.7. Ensure benchmark validity

This is where validity concepts come in.

## A. Internal validity
Question:

> Does the observed difference truly come from the variable I intended to study?

Example:
if you change context, prompt, and judge simultaneously, you do not know what caused what.

## B. External validity
Question:

> Does this generalize outside of this benchmark?

Example:
a strong result on synthetic graphs does not imply the same result on real agent tasks.

## C. Construct validity
Question:

> Does the metric I use truly represent the phenomenon I claim to measure?

Example:
if `reason_preserved` accepts distractor responses, then it does not properly measure "correct reason preservation".

## D. Conclusion validity
Question:

> Are the statistical conclusions justified by the design and the analysis?

Example:
if there is only one execution per cell, you cannot assert strong statistical robustness.

---

## 3.8. Data quality inspection before analysis

Before calculating means or testing hypotheses, the data must be validated.

### Minimum checks
- expected total number of rows
- distribution per condition
- absence of empty cells
- absence of accidental duplicates
- detection of impossible values
- label consistency
- traceability between run and experimental condition

### Also review:
- logs
- concrete examples
- individual outputs
- not only aggregates

This is critical.

Many methodological errors are detected by examining concrete benchmark examples, not just summary tables.

---

## 3.9. Perform descriptive analysis first

Before running tests or drawing conclusions, descriptive analysis must be performed.

### What it includes
- means
- medians
- distributions
- tables by factor
- cross-tabulations
- visualization of strong patterns

### Objective
Understand the global behavior of the data before interpreting causality.

### Important
Descriptive analysis does not demonstrate causality.
It only shows patterns.

---

## 3.10. Analyze uncertainty

This is one of the differences between a "dashboard" and a "scientific analysis".

Reporting means is not enough.
Uncertainty must be reported.

### Common tools
- standard deviation
- standard error
- confidence intervals
- distribution per replica
- bootstrap
- Bayesian analysis if applicable

### Important warning
A confidence interval over a single aggregated sample **does not substitute** for the lack of experimental replication.

Example:
if you have a single observation per cell, the interval over the global rate may be mathematically correct, but it tells you nothing solid about stability across executions.

---

## 3.11. Analyze confounding factors

You must always ask:

> Could there be another explanation for this result?

Examples of confounding in your type of benchmark:
- prompt more important than context
- permissive judge
- noise that introduces useful hints
- non-comparable agent/judge pairs
- poorly balanced dataset
- semantically contaminated distractors

Scientific analysis requires attempting to falsify the most comfortable explanation.

---

## 3.12. Perform hypothesis testing with caution

Only after all of the above does formal testing make sense.

### What to compare
- explanatory vs structural
- mixed vs structural
- planner A vs planner B
- prompt X vs prompt Y

### What you need to do it properly
- sufficient replication
- well-defined metrics
- clean design
- reasonable assumptions

### If you do not have that
That is fine, but then you must speak of:

- exploratory evidence,
- not strong proof.

---

## 3.13. Clearly distinguish results, interpretation, and conclusion

This separation is critical.

### Result
Observed datum.

Example:
- "The average `restart` for `explanatory` is 0.458 and for `structural` is 0.188".

### Interpretation
Reasonable reading of the datum.

Example:
- "This suggests that explanatory context facilitates locating the resumption point".

### Conclusion
Final statement with controlled scope.

Example:
- "In this synthetic benchmark, explanatory outperforms structural in restart, but the absence of replication limits the robustness of the claim".

The typical mistake is jumping from the result to an overly ambitious conclusion.

---

## 3.14. Communicate limitations explicitly

Every serious report must clearly state what it does not demonstrate.

### Must include
- dataset limitations
- design limitations
- metric limitations
- generalization limitations
- possible judge biases
- sample size limits
- pending methodological debt

This does not weaken the work.
It strengthens it, because it makes it credible.

---

## 4. Practical application to agent and rehydration benchmarks

In a benchmark like yours, the correct procedure would be:

### Step 1
Define concrete questions:

- Does explanatory context improve operational continuity?
- Does the multi-resolution bundle improve causal preservation under token pressure?
- Does `ResumeFocused` mode improve resumption?

### Step 2
Define independent variables:

- context mix
- planner mode
- prompt
- agent model
- judge model
- budget
- noise mode
- graph scale

### Step 3
Define better metrics:

- `task_success`
- `restart_exact`
- `restart_off_by_one`
- `reason_correct_main_path`
- `reason_plausible_but_wrong`
- `token_efficiency`
- `bundle_stability`

### Step 4
Create a benchmark without semantic contamination

For example:

- `structural` truly without textual rationale
- `competing_restart`
- `conflicting_reason`
- `irrelevant_noise`

### Step 5
Replicate each condition

At a minimum:
- 3 seeds per cell

### Step 6
Perform descriptive analysis

### Step 7
Perform cautious inferential analysis

### Step 8
Report:
- results
- uncertainty
- limitations
- interpretation
- and next benchmark corrections

---

## 5. Mistakes to always avoid

## Mistake 1
Confusing many rows with good science.

Having 720 rows does not imply good evidence if the experimental structure has a single observation per condition.

## Mistake 2
Confusing pretty charts with robustness.

Many tables and charts help with readability, but they do not substitute for:
- replication,
- variable control,
- or good metric definition.

## Mistake 3
Accepting a poorly defined metric.

An ambiguous metric contaminates all subsequent analysis.

## Mistake 4
Not inspecting concrete examples.

Looking only at aggregates prevents detecting:
- leakage,
- semantic contamination,
- permissive judge,
- labeling errors.

## Mistake 5
Overclaiming.

The correct phrasing many times is:
- "suggests"
- "points to"
- "shows directional evidence"
- "does not yet demonstrate robustness"

That is honest science.

---

## 6. Practical template for future reports

## A. Research question
One clear sentence.

## B. Hypothesis
Main hypothesis and null hypothesis.

## C. Experimental design
Factors, levels, cells, number of repetitions.

## D. Metrics
Exact definition of each metric.

## E. Data quality checks
Prior integrity validation.

## F. Descriptive results
Tables and figures.

## G. Uncertainty analysis
Replication, variance, intervals.

## H. Threats to validity
Internal, external, construct, conclusion.

## I. Interpretation
Cautious reading.

## J. Conclusion
What can be claimed and what cannot.

---

## 7. Quality criterion for saying "this is now strong evidence"

I would not say a benchmark offers strong evidence until it meets, at a minimum, the following:

1. clean and well-separated conditions;
2. unambiguous metrics;
3. replication per cell;
4. design without evident contamination;
5. review of individual outputs;
6. uncertainty analysis;
7. explicit limitations;
8. conclusions proportionate to the design.

If any of the first three are missing, the benchmark is still in an exploratory phase.

---

## 8. Final conclusion

The correct scientific procedure does not consist of accumulating numbers.
It consists of building reliable, traceable, and falsifiable evidence.

Applied to rehydration and agent benchmarks, this requires:

- designing the benchmark well,
- defining metrics well,
- replicating,
- reviewing concrete examples,
- analyzing uncertainty,
- and communicating with methodological humility.

The most useful practical rule is this:

> **first clean the benchmark, then measure, then interpret, and only at the end conclude.**

That order is not bureaucracy.
It is what separates a useful experiment from a narrative based on numbers.
