#!/usr/bin/env python3
"""
Extract exhaustive E2E evaluation report from a run directory.

Usage:
  python3 scripts/ci/extract-e2e-report.py [run_dir]

  run_dir defaults to the latest artifacts/e2e-runs/*/ directory.

Reads:
  - results/*.json     (per-evaluation structured data)
  - test.log           (full stderr with inference responses and judge verdicts)

Produces:
  - <run_dir>/report-full.md   (exhaustive markdown with all tables and analysis)
"""

import json
import glob
import math
import os
import sys
from collections import defaultdict, OrderedDict
from pathlib import Path

# ── Table separator/header constants (S1192: deduplicate literals) ──
SEP_CROSS_MIX = "|-------|-------------|------------|-------|"
HDR_AGENT_MIX = "| Agent | Explanatory | Structural | Mixed |"
SEP_METRIC_EVALS = "|-------|------|---------|--------|-------|"
HDR_MIX_CI = "| Mix | N | Rate | 95% CI |"
SEP_MIX_CI = "|-----|---|------|--------|"
HDR_PROMPT = "| Prompt | Task | Restart | Reason | Evals |"
SEP_PROMPT = "|--------|------|---------|--------|-------|"
HDR_DOMAIN = "| Domain | Task | Restart | Reason | Evals |"
SEP_DOMAIN = "|--------|------|---------|--------|-------|"
HDR_NOISE = "| Noise | Task | Restart | Reason Correct | Reason Distractor | Evals |"
SEP_NOISE = "|-------|------|---------|----------------|-------------------|-------|"
HDR_RESTART = "| Noise | Restart | Exact | Off-by-1 | Competing | Explained | Evals |"
SEP_RESTART = "|-------|---------|-------|----------|-----------|-----------|-------|"
HDR_OVERVIEW = "| Metric | OK | FAIL | ERR | Rate |"
SEP_OVERVIEW = "|--------|----|------|-----|------|"
HDR_AGENT_PROMPT = "| Agent | Judge | Prompt | Task | Restart | Reason | Avg Latency |"
SEP_AGENT_PROMPT = "|-------|-------|--------|------|---------|--------|-------------|"
HDR_AGENT_GROUP = "| Agent | Task | Restart | Reason | Evals |"
HDR_JUDGE_GROUP = "| Judge | Task | Restart | Reason | Evals |"
HDR_VARIANT_MIX = "| Mix | Task | Restart | Reason Correct | Reason Distractor | Evals |"
SEP_VARIANT_MIX = "|-----|------|---------|----------------|-------------------|-------|"
HDR_SCALE = "| Scale | Task | Restart | Reason | Evals |"
HDR_HEATMAP = "| Noise | Explanatory | Structural | Mixed |"
HDR_SCALE_HEATMAP = "| Scale | Explanatory | Structural | Mixed |"
HDR_AGENT_COMPARE = "| Agent | Evals | Task | Restart | Reason Correct | Reason Distractor |"
SEP_AGENT_COMPARE = "|-------|-------|------|---------|----------------|-------------------|"
HDR_JUDGE_COMPARE = "| Judge | Evals | Task | Restart | Reason Correct | Reason Distractor |"
SEP_JUDGE_COMPARE = "|-------|-------|------|---------|----------------|-------------------|"
HDR_CONVERGENCE = "| Agent→Judge | Metric | Strict | Lenient | Default | Delta S-L |"
SEP_CONVERGENCE = "|-------------|--------|--------|---------|---------|-----------|"
HDR_LATENCY_AGENT = "| Agent | Avg Latency | Min | Max | P50 |"
SEP_LATENCY_AGENT = "|-------|-------------|-----|-----|-----|"
HDR_COST = "| Role | Model | Evals | Est. Cost |"
SEP_COST = "|------|-------|-------|-----------|"
HDR_AGENT_CI = "| Agent | N | Rate | 95% CI |"
SEP_AGENT_CI = "|-------|---|------|--------|"
HDR_LATENCY_CI = "| Agent | N | Mean | Std | P25 | P50 | P75 | P95 |"
SEP_LATENCY_CI = "|-------|---|------|-----|-----|-----|-----|-----|"
HDR_VARIANCE = "| Condition | Seeds | Task rates | Task variance |"
SEP_VARIANCE = "|-----------|-------|------------|---------------|"
HDR_TOKEN = "| Mix | Avg Rendered | Avg Raw | Avg Compression | Causal Density | Detail Coverage |"
SEP_TOKEN = "|-----|-------------|---------|-----------------|----------------|-----------------|"
HDR_EVALS = "| # | Agent | Judge | Prompt | Variant | Task | Restart | Reason | Rc | Rd | Latency | Failure Point | Restart Node |"
SEP_EVALS = "|---|-------|-------|--------|---------|------|---------|--------|----|----|---------|---------------|--------------|"


def find_latest_run():
    base = Path(__file__).resolve().parent.parent.parent / "artifacts" / "e2e-runs"
    runs = sorted(base.glob("*/"), key=lambda p: p.name, reverse=True)
    if not runs:
        print("No run directories found in artifacts/e2e-runs/", file=sys.stderr)
        sys.exit(1)
    return runs[0]


def load_results(run_dir):
    files = sorted(glob.glob(str(run_dir / "results" / "*.json")))
    results = []
    for f in files:
        with open(f) as fh:
            d = json.load(fh)
            d["_file"] = os.path.basename(f)
            results.append(d)
    return results


def parse_variant(variant):
    """Parse 'micro-ops-explanatory-clean' or 'micro-ops-explanatory-clean-s0' into components."""
    parts = variant.split("-", 3)
    if len(parts) >= 4:
        noise = parts[3]
        seed = "0"
        # Strip seed suffix like '-s0', '-s1'
        if "-s" in noise:
            noise, seed = noise.rsplit("-s", 1)
        return {"scale": parts[0], "domain": parts[1], "mix": parts[2], "noise": noise, "seed": seed}
    return {"scale": "?", "domain": "?", "mix": "?", "noise": "?", "seed": "0"}


def parse_model(model):
    """Parse 'qwen3-8b\u2192opus-4.6' into agent and judge."""
    if "\u2192" in model:
        parts = model.split("\u2192", 1)
        return parts[0], parts[1]
    return model, "?"


def ratio(ok, total):
    if total == 0:
        return "n/a"
    pct = 100.0 * ok / total
    return f"{ok}/{total} ({pct:.0f}%)"


def avg_latency(results):
    latencies = [r["latency_ms"] for r in results if r.get("latency_ms", 0) > 0]
    if not latencies:
        return "n/a"
    return f"{sum(latencies) / len(latencies):.0f}ms"


def fig_ref(run_dir, filename, caption=""):
    """Return markdown image reference if figure exists, else empty string."""
    fig_path = run_dir / "figures" / filename
    if fig_path.exists():
        alt = caption or filename.replace(".png", "").replace("_", " ")
        return f"\n![{alt}](figures/{filename})\n"
    return ""


def tristate_csv(value):
    """Convert a True/False/None value to 'true'/'false'/'null' for CSV export."""
    if value is True:
        return "true"
    if value is False:
        return "false"
    return "null"


def tristate_label(value, true_label="OK", false_label="FAIL", none_label="ERR"):
    """Convert a True/False/None value to display labels."""
    if value is True:
        return true_label
    if value is False:
        return false_label
    return none_label


def wilson_ci(ok, n):
    """Wilson score interval for binomial proportion."""
    if n == 0:
        return 0.0, 0.0, 0.0
    z = 1.96
    p_hat = ok / n
    denom = 1 + z * z / n
    center = (p_hat + z * z / (2 * n)) / denom
    spread = z * math.sqrt((p_hat * (1 - p_hat) + z * z / (4 * n)) / n) / denom
    lo = max(0.0, center - spread)
    hi = min(1.0, center + spread)
    return p_hat, lo, hi


def count_true(rows, key):
    return sum(1 for row in rows if row.get(key) is True)


def count_truthy(rows, key):
    return sum(1 for row in rows if row.get(key))


def count_rows(rows):
    return len(rows)


def append_row(md, values):
    md.append(f"| {' | '.join(values)} |")


def ratio_row(label, rows, key, total=None):
    total = len(rows) if total is None else total
    ok = count_true(rows, key)
    return [label, str(ok), str(total - ok), str(0), ratio(ok, total)]


def _parse_results(results):
    """Pre-compute groupings used across multiple report sections."""
    by_cell = defaultdict(list)
    by_agent = defaultdict(list)
    by_judge = defaultdict(list)
    by_prompt = defaultdict(list)
    by_mix = defaultdict(list)
    by_scale = defaultdict(list)
    by_domain = defaultdict(list)
    by_noise = defaultdict(list)

    for r in results:
        agent, judge = parse_model(r["model"])
        by_cell[(agent, judge, r["prompt"])].append(r)
        by_agent[agent].append(r)
        by_judge[judge].append(r)
        by_prompt[r["prompt"]].append(r)
        v = parse_variant(r["variant"])
        by_mix[v["mix"]].append(r)
        by_scale[v["scale"]].append(r)
        by_domain[v["domain"]].append(r)
        by_noise[v["noise"]].append(r)

    return by_cell, by_agent, by_judge, by_prompt, by_mix, by_scale, by_domain, by_noise


def _build_overview(md, results):
    """Append the overview summary table."""
    total = len(results)
    task_ok = count_true(results, "task")
    task_fail = sum(1 for r in results if r.get("task") is False)
    task_err = count_rows([r for r in results if r.get("task") is None])
    restart_ok = count_true(results, "restart")
    restart_exact = count_true(results, "restart_exact")
    restart_off1 = count_true(results, "restart_off_by_one")
    restart_competing = count_true(results, "restart_on_competing")
    restart_explained = count_true(results, "restart_explained")
    reason_correct_ok = count_true(results, "reason_correct")
    reason_distractor_ok = count_true(results, "reason_distractor")

    md.append("## Overview")
    md.append("")
    md.append(HDR_OVERVIEW)
    md.append(SEP_OVERVIEW)
    md.append(f"| Task | {task_ok} | {task_fail} | {task_err} | {ratio(task_ok, total)} |")
    md.append(f"| Restart | {restart_ok} | {total - restart_ok - task_err} | {task_err} | {ratio(restart_ok, total)} |")
    md.append(f"| \u21b3 Exact | {restart_exact} | | | {ratio(restart_exact, total)} |")
    md.append(f"| \u21b3 Off-by-one | {restart_off1} | | | {ratio(restart_off1, total)} |")
    md.append(f"| \u21b3 Competing branch | {restart_competing} | | | {ratio(restart_competing, total)} |")
    md.append(f"| \u21b3 Explained | {restart_explained} | | | {ratio(restart_explained, total)} |")
    md.append(f"| Reason Correct | {reason_correct_ok} | {total - reason_correct_ok - task_err} | {task_err} | {ratio(reason_correct_ok, total)} |")
    md.append(f"| Reason Distractor | {reason_distractor_ok} | | | {ratio(reason_distractor_ok, total)} |")
    md.append("")


def _build_summary_tables(md, run_dir, by_cell, by_agent, by_judge, by_prompt):
    """Append per-agent, per-judge, per-prompt, and agent x judge x prompt tables."""
    # ── By Agent x Judge x Prompt ──
    md.append("## By Agent x Judge x Prompt")
    md.append("")
    md.append(HDR_AGENT_PROMPT)
    md.append(SEP_AGENT_PROMPT)

    for (agent, judge, prompt), cell in sorted(by_cell.items()):
        n = len(cell)
        t = sum(1 for r in cell if r.get("task") is True)
        re = sum(1 for r in cell if r.get("restart") is True)
        p = sum(1 for r in cell if r.get("reason") is True)
        lat = avg_latency(cell)
        append_row(md, [agent, judge, prompt, ratio(t, n), ratio(re, n), ratio(p, n), lat])

    md.append("")

    # ── By Agent (aggregated across judges and prompts) ──
    md.append("## By Agent (all judges, all prompts)")
    md.append("")
    md.append(HDR_AGENT_GROUP)
    md.append(SEP_METRIC_EVALS)

    for agent, cell in sorted(by_agent.items()):
        n = len(cell)
        t = sum(1 for r in cell if r.get("task") is True)
        re = sum(1 for r in cell if r.get("restart") is True)
        p = sum(1 for r in cell if r.get("reason") is True)
        append_row(md, [agent, ratio(t, n), ratio(re, n), ratio(p, n), str(n)])

    md.append("")

    # ── By Judge (aggregated) ──
    md.append("## By Judge (all agents, all prompts)")
    md.append("")
    md.append(HDR_JUDGE_GROUP)
    md.append(SEP_METRIC_EVALS)

    for judge, cell in sorted(by_judge.items()):
        n = len(cell)
        t = sum(1 for r in cell if r.get("task") is True)
        re = sum(1 for r in cell if r.get("restart") is True)
        p = sum(1 for r in cell if r.get("reason") is True)
        append_row(md, [judge, ratio(t, n), ratio(re, n), ratio(p, n), str(n)])

    md.append("")

    # ── By Prompt (aggregated) ──
    md.append("## By Prompt Variant")
    md.append(fig_ref(run_dir, "05_prompt_comparison.png", "Prompt Variant Effectiveness"))
    md.append("")
    md.append(HDR_PROMPT)
    md.append(SEP_PROMPT)

    for prompt, cell in sorted(by_prompt.items()):
        n = len(cell)
        t = sum(1 for r in cell if r.get("task") is True)
        re = sum(1 for r in cell if r.get("restart") is True)
        p = sum(1 for r in cell if r.get("reason") is True)
        append_row(md, [prompt, ratio(t, n), ratio(re, n), ratio(p, n), str(n)])

    md.append("")


def _build_dimension_tables(md, run_dir, by_mix, by_scale, by_domain, by_noise):
    """Append tables for relation mix, scale, domain, and noise mode."""
    # ── By Relation Mix (THE key signal) ──
    md.append("## By Relation Mix (key signal)")
    md.append(fig_ref(run_dir, "01_relation_mix_bars.png", "Rehydration Quality by Relation Type"))
    md.append("")
    md.append(HDR_VARIANT_MIX)
    md.append(SEP_VARIANT_MIX)

    for mix in ["explanatory", "structural", "mixed"]:
        cell = by_mix.get(mix, [])
        if not cell:
            continue
        n = len(cell)
        t = sum(1 for r in cell if r.get("task") is True)
        re = sum(1 for r in cell if r.get("restart") is True)
        rc = sum(1 for r in cell if r.get("reason_correct") is True)
        rd = sum(1 for r in cell if r.get("reason_distractor") is True)
        append_row(md, [f"**{mix}**", f"**{ratio(t, n)}**", f"**{ratio(re, n)}**", f"**{ratio(rc, n)}**", f"**{ratio(rd, n)}**", str(n)])

    md.append("")

    # ── By Scale ──
    md.append("## By Scale")
    md.append(fig_ref(run_dir, "04_scale_effect.png", "Scale Effect"))
    md.append("")
    md.append(HDR_SCALE)
    md.append(SEP_METRIC_EVALS)

    for scale in ["micro", "meso", "stress"]:
        cell = by_scale.get(scale, [])
        if not cell:
            continue
        n = len(cell)
        t = sum(1 for r in cell if r.get("task") is True)
        re = sum(1 for r in cell if r.get("restart") is True)
        p = sum(1 for r in cell if r.get("reason") is True)
        append_row(md, [scale, ratio(t, n), ratio(re, n), ratio(p, n), str(n)])

    md.append("")

    # ── By Domain ──
    md.append("## By Domain")
    md.append("")
    md.append(HDR_DOMAIN)
    md.append(SEP_DOMAIN)

    for domain, cell in sorted(by_domain.items()):
        n = len(cell)
        t = sum(1 for r in cell if r.get("task") is True)
        re = sum(1 for r in cell if r.get("restart") is True)
        p = sum(1 for r in cell if r.get("reason") is True)
        append_row(md, [domain, ratio(t, n), ratio(re, n), ratio(p, n), str(n)])

    md.append("")

    # ── By Noise Mode ──
    md.append("## By Noise Mode")
    md.append(fig_ref(run_dir, "07_noise_impact.png", "Noise Impact"))
    md.append("")
    md.append(HDR_NOISE)
    md.append(SEP_NOISE)

    for noise in ["clean", "competing", "conflicting", "restart"]:
        cell = by_noise.get(noise, [])
        if not cell:
            continue
        n = len(cell)
        t = sum(1 for r in cell if r.get("task") is True)
        re = sum(1 for r in cell if r.get("restart") is True)
        rc = sum(1 for r in cell if r.get("reason_correct") is True)
        rd = sum(1 for r in cell if r.get("reason_distractor") is True)
        append_row(md, [f"**{noise}**", ratio(t, n), ratio(re, n), ratio(rc, n), ratio(rd, n), str(n)])

    # Any noise modes not in the predefined list
    for noise, cell in sorted(by_noise.items()):
        if noise in ("clean", "competing", "conflicting", "restart"):
            continue
        n = len(cell)
        t = sum(1 for r in cell if r.get("task") is True)
        re = sum(1 for r in cell if r.get("restart") is True)
        rc = sum(1 for r in cell if r.get("reason_correct") is True)
        rd = sum(1 for r in cell if r.get("reason_distractor") is True)
        append_row(md, [noise, ratio(t, n), ratio(re, n), ratio(rc, n), ratio(rd, n), str(n)])

    md.append("")


def _build_heatmaps(md, run_dir, results, by_agent, by_noise):
    """Append cross-dimension heatmap tables (noise x mix, agent x mix, scale x mix)."""
    # ── Cross: Noise x Mix ──
    md.append("## Noise x Relation Mix (Task)")
    md.append("")
    md.append(HDR_HEATMAP)
    md.append(SEP_CROSS_MIX)

    for noise in ["clean", "competing", "conflicting", "restart"]:
        cell = by_noise.get(noise, [])
        if not cell:
            continue
        row = [noise]
        for mix in ["explanatory", "structural", "mixed"]:
            mix_cell = [r for r in cell if parse_variant(r["variant"])["mix"] == mix]
            n = len(mix_cell)
            t = sum(1 for r in mix_cell if r.get("task") is True)
            row.append(ratio(t, n))
        append_row(md, row)

    md.append("")

    # ── Cross: Agent x Mix (most important table) ──
    md.append("## Agent x Relation Mix (Task)")
    md.append(fig_ref(run_dir, "02_agent_x_mix_task.png", "Task Identification by Agent and Relation Type"))
    md.append("")
    md.append(HDR_AGENT_MIX)
    md.append(SEP_CROSS_MIX)

    for agent in sorted(by_agent.keys()):
        row = [agent]
        for mix in ["explanatory", "structural", "mixed"]:
            cell = [r for r in by_agent[agent] if parse_variant(r["variant"])["mix"] == mix]
            n = len(cell)
            t = sum(1 for r in cell if r.get("task") is True)
            row.append(ratio(t, n))
        append_row(md, row)

    md.append("")

    # ── Cross: Agent x Mix (Reason Correct) ──
    md.append("## Agent x Relation Mix (Reason Correct)")
    md.append(fig_ref(run_dir, "03_agent_x_mix_reason.png", "Rationale Preservation by Agent and Relation Type"))
    md.append("")
    md.append(HDR_AGENT_MIX)
    md.append(SEP_CROSS_MIX)

    for agent in sorted(by_agent.keys()):
        row = [agent]
        for mix in ["explanatory", "structural", "mixed"]:
            cell = [r for r in by_agent[agent] if parse_variant(r["variant"])["mix"] == mix]
            n = len(cell)
            p = sum(1 for r in cell if r.get("reason_correct") is True)
            row.append(ratio(p, n))
        append_row(md, row)

    md.append("")

    # ── Cross: Agent x Mix (Reason Distractor) ──
    md.append("## Agent x Relation Mix (Reason Distractor)")
    md.append("")
    md.append(HDR_AGENT_MIX)
    md.append(SEP_CROSS_MIX)

    for agent in sorted(by_agent.keys()):
        row = [agent]
        for mix in ["explanatory", "structural", "mixed"]:
            cell = [r for r in by_agent[agent] if parse_variant(r["variant"])["mix"] == mix]
            n = len(cell)
            d = sum(1 for r in cell if r.get("reason_distractor") is True)
            row.append(ratio(d, n))
        append_row(md, row)

    md.append("")

    # ── Cross: Scale x Mix (Task) ──
    md.append("## Scale x Relation Mix (Task)")
    md.append(fig_ref(run_dir, "04_scale_effect.png", "Scale Effect on Task Identification"))
    md.append("")
    md.append(HDR_SCALE_HEATMAP)
    md.append(SEP_CROSS_MIX)

    for scale in ["micro", "meso", "stress"]:
        row = [scale]
        for mix in ["explanatory", "structural", "mixed"]:
            cell = [r for r in results
                    if parse_variant(r["variant"])["scale"] == scale
                    and parse_variant(r["variant"])["mix"] == mix]
            n = len(cell)
            t = sum(1 for r in cell if r.get("task") is True)
            row.append(ratio(t, n))
        append_row(md, row)

    md.append("")


def _build_controlled_comparisons(md, results, by_agent, by_judge):
    """Append controlled agent-vs-judge comparison tables."""
    # ── Controlled comparison: Agent (fixed judge) ──
    md.append("## Controlled: Agent Comparison (fixed judge)")
    md.append("")
    md.append("Each table holds the judge constant, enabling fair agent comparison.")
    md.append("Evals per cell are equal when judge is fixed.")
    md.append("")

    for judge in sorted(by_judge.keys()):
        judge_results = [r for r in results if parse_model(r["model"])[1] == judge]
        if not judge_results:
            continue
        agents_in_judge = sorted({parse_model(r["model"])[0] for r in judge_results})
        md.append(f"### Judge = {judge}")
        md.append("")
        md.append(HDR_AGENT_COMPARE)
        md.append(SEP_AGENT_COMPARE)
        for agent in agents_in_judge:
            cell = [r for r in judge_results if parse_model(r["model"])[0] == agent]
            n = len(cell)
            t = sum(1 for r in cell if r.get("task") is True)
            re = sum(1 for r in cell if r.get("restart") is True)
            rc = sum(1 for r in cell if r.get("reason_correct") is True)
            rd = sum(1 for r in cell if r.get("reason_distractor") is True)
            append_row(md, [agent, str(n), ratio(t, n), ratio(re, n), ratio(rc, n), ratio(rd, n)])
        md.append("")

    # ── Controlled comparison: Judge (fixed agent) ──
    md.append("## Controlled: Judge Comparison (fixed agent)")
    md.append("")
    md.append("Each table holds the agent constant, enabling fair judge comparison.")
    md.append("")

    for agent in sorted(by_agent.keys()):
        agent_results = [r for r in results if parse_model(r["model"])[0] == agent]
        if not agent_results:
            continue
        judges_in_agent = sorted({parse_model(r["model"])[1] for r in agent_results})
        if len(judges_in_agent) < 2:
            continue
        md.append(f"### Agent = {agent}")
        md.append("")
        md.append(HDR_JUDGE_COMPARE)
        md.append(SEP_JUDGE_COMPARE)
        for judge in judges_in_agent:
            cell = [r for r in agent_results if parse_model(r["model"])[1] == judge]
            n = len(cell)
            t = sum(1 for r in cell if r.get("task") is True)
            re = sum(1 for r in cell if r.get("restart") is True)
            rc = sum(1 for r in cell if r.get("reason_correct") is True)
            rd = sum(1 for r in cell if r.get("reason_distractor") is True)
            append_row(md, [judge, str(n), ratio(t, n), ratio(re, n), ratio(rc, n), ratio(rd, n)])
        md.append("")


def _build_convergence_table(md, run_dir, by_cell):
    """Append prompt convergence (strict vs lenient) table."""
    md.append("## Prompt Convergence: Strict vs Lenient")
    md.append(fig_ref(run_dir, "09_convergence_heatmap.png", "Convergence Heatmap"))
    md.append("")
    md.append(HDR_CONVERGENCE)
    md.append(SEP_CONVERGENCE)

    for (agent, judge) in sorted({(a, j) for (a, j, _) in by_cell.keys()}):
        for metric_name, metric_key in [("Task", "task"), ("Restart", "restart"), ("Reason", "reason")]:
            vals = {}
            for prompt in ["strict-judge", "lenient-judge", "default"]:
                cell = by_cell.get((agent, judge, prompt), [])
                n = len(cell)
                ok = sum(1 for r in cell if r.get(metric_key) is True)
                vals[prompt] = (ok, n)
            s_ok, s_n = vals.get("strict-judge", (0, 0))
            l_ok, l_n = vals.get("lenient-judge", (0, 0))
            d_ok, d_n = vals.get("default", (0, 0))
            if s_n > 0 and l_n > 0:
                delta = abs(s_ok - l_ok)
            else:
                delta = "?"
            append_row(md, [f"{agent}\u2192{judge}", metric_name, ratio(s_ok, s_n), ratio(l_ok, l_n), ratio(d_ok, d_n), str(delta)])

    md.append("")


def _build_latency_and_cost(md, run_dir, results, by_agent, by_judge):
    """Append latency and cost estimate tables."""
    # ── Latency by agent ──
    md.append("## Latency by Agent")
    md.append(fig_ref(run_dir, "08_latency_boxplot.png", "Latency Distribution"))
    md.append("")
    md.append(HDR_LATENCY_AGENT)
    md.append(SEP_LATENCY_AGENT)

    for agent, cell in sorted(by_agent.items()):
        latencies = sorted([r["latency_ms"] for r in cell if r.get("latency_ms", 0) > 0])
        if not latencies:
            continue
        avg = sum(latencies) / len(latencies)
        p50 = latencies[len(latencies) // 2]
        append_row(md, [agent, f"{avg:.0f}ms", f"{latencies[0]:.0f}ms", f"{latencies[-1]:.0f}ms", f"{p50:.0f}ms"])

    md.append("")

    # ── Cost estimate ──
    md.append("## Cost Estimate")
    md.append("")

    prices = {
        "qwen3-8b": {"input": 0, "output": 0},
        "gpt-5.4": {"input": 2.50, "output": 10.0},
        "opus-4.6": {"input": 15.0, "output": 75.0},
    }
    input_per_call = 700
    total_cost = 0.0

    md.append(HDR_COST)
    md.append(SEP_COST)

    for agent, cell in sorted(by_agent.items()):
        n = len(cell)
        out_tokens = sum(len(r.get("agent_response", "")) // 4 for r in cell)
        p = prices.get(agent, {"input": 0, "output": 0})
        cost = (n * input_per_call * p["input"] + out_tokens * p["output"]) / 1_000_000
        total_cost += cost
        append_row(md, ["agent", agent, str(n), f"${cost:.2f}"])

    for judge, cell in sorted(by_judge.items()):
        n = len(cell)
        out_tokens = sum(len(r.get("judge_raw", "") or "") // 4 for r in cell)
        p = prices.get(judge, {"input": 0, "output": 0})
        cost = (n * 600 * p["input"] + out_tokens * p["output"]) / 1_000_000
        total_cost += cost
        append_row(md, ["judge", judge, str(n), f"${cost:.2f}"])

    append_row(md, ["**total**", "", f"**{len(results)}**", f"**${total_cost:.2f}**"])
    md.append("")


def _build_restart_diagnostics(md, by_noise):
    """Append restart diagnostic table by noise mode."""
    md.append("## Restart Diagnostic by Noise Mode")
    md.append("")
    md.append(HDR_RESTART)
    md.append(SEP_RESTART)

    for noise in ["clean", "competing", "conflicting", "restart"]:
        cell = by_noise.get(noise, [])
        if not cell:
            continue
        n = len(cell)
        re = sum(1 for r in cell if r.get("restart") is True)
        rx = sum(1 for r in cell if r.get("restart_exact") is True)
        ro = sum(1 for r in cell if r.get("restart_off_by_one") is True)
        rcb = sum(1 for r in cell if r.get("restart_on_competing") is True)
        rexp = sum(1 for r in cell if r.get("restart_explained") is True)
        append_row(md, [f"**{noise}**", ratio(re, n), ratio(rx, n), ratio(ro, n), ratio(rcb, n), ratio(rexp, n), str(n)])

    md.append("")


def _build_statistical_summary(md, run_dir, by_agent, by_mix):
    """Append statistical summary with Wilson CIs and latency distributions."""
    md.append("## Statistical Summary")
    md.append(fig_ref(run_dir, "10_judge_strictness.png", "Judge Strictness"))
    md.append(fig_ref(run_dir, "15_inter_rater_agreement.png", "Inter-Rater Agreement"))
    md.append("")
    md.append("Success rates with 95% confidence intervals (Wilson score).")
    md.append("")

    md.append("### Task Success Rate by Relation Mix")
    md.append("")
    md.append(HDR_MIX_CI)
    md.append(SEP_MIX_CI)
    for mix in ["explanatory", "structural", "mixed"]:
        cell = by_mix.get(mix, [])
        n = len(cell)
        ok = sum(1 for r in cell if r.get("task") is True)
        rate, lo, hi = wilson_ci(ok, n)
        append_row(md, [mix, str(n), f"{rate:.1%}", f"[{lo:.1%}, {hi:.1%}]"])
    md.append("")

    md.append("### Reason Correct (Main Path) Rate by Relation Mix")
    md.append("")
    md.append(HDR_MIX_CI)
    md.append(SEP_MIX_CI)
    for mix in ["explanatory", "structural", "mixed"]:
        cell = by_mix.get(mix, [])
        n = len(cell)
        ok = sum(1 for r in cell if r.get("reason_correct") is True)
        rate, lo, hi = wilson_ci(ok, n)
        append_row(md, [mix, str(n), f"{rate:.1%}", f"[{lo:.1%}, {hi:.1%}]"])
    md.append("")

    md.append("### Reason Distractor Leakage Rate by Relation Mix")
    md.append("")
    md.append(HDR_MIX_CI)
    md.append(SEP_MIX_CI)
    for mix in ["explanatory", "structural", "mixed"]:
        cell = by_mix.get(mix, [])
        n = len(cell)
        ok = sum(1 for r in cell if r.get("reason_distractor") is True)
        rate, lo, hi = wilson_ci(ok, n)
        append_row(md, [mix, str(n), f"{rate:.1%}", f"[{lo:.1%}, {hi:.1%}]"])
    md.append("")

    md.append("### Task Success Rate by Agent")
    md.append("")
    md.append(HDR_AGENT_CI)
    md.append(SEP_AGENT_CI)
    for agent in sorted(by_agent.keys()):
        cell = by_agent[agent]
        n = len(cell)
        ok = sum(1 for r in cell if r.get("task") is True)
        rate, lo, hi = wilson_ci(ok, n)
        append_row(md, [agent, str(n), f"{rate:.1%}", f"[{lo:.1%}, {hi:.1%}]"])
    md.append("")

    md.append("### Latency Distribution by Agent (ms)")
    md.append("")
    md.append(HDR_LATENCY_CI)
    md.append(SEP_LATENCY_CI)
    for agent in sorted(by_agent.keys()):
        latencies = sorted([r["latency_ms"] for r in by_agent[agent] if r.get("latency_ms", 0) > 0])
        if not latencies:
            continue
        n = len(latencies)
        mean = sum(latencies) / n
        std = math.sqrt(sum((x - mean) ** 2 for x in latencies) / n)
        p25 = latencies[int(n * 0.25)]
        p50 = latencies[int(n * 0.50)]
        p75 = latencies[int(n * 0.75)]
        p95 = latencies[int(n * 0.95)]
        append_row(md, [agent, str(n), f"{mean:.0f}", f"{std:.0f}", f"{p25:.0f}", f"{p50:.0f}", f"{p75:.0f}", f"{p95:.0f}"])
    md.append("")


def _export_csv(run_dir, results):
    """Write per-evaluation CSV file."""
    csv_path = run_dir / "results.csv"
    with open(csv_path, "w") as f:
        f.write("eval_num,agent,judge,prompt,variant,scale,domain,mix,noise,task,restart,restart_exact,restart_off_by_one,restart_on_competing,restart_explained,reason,reason_correct,reason_distractor,latency_ms,failure_point,restart_node\n")
        for i, r in enumerate(results, 1):
            agent, judge = parse_model(r["model"])
            v = parse_variant(r["variant"])
            t = tristate_csv(r.get("task"))
            re = tristate_csv(r.get("restart"))
            p = tristate_csv(r.get("reason"))
            rx = tristate_csv(r.get("restart_exact"))
            ro = tristate_csv(r.get("restart_off_by_one"))
            rcb = tristate_csv(r.get("restart_on_competing"))
            rexp = tristate_csv(r.get("restart_explained"))
            rc = tristate_csv(r.get("reason_correct"))
            rd = tristate_csv(r.get("reason_distractor"))
            lat = f"{r.get('latency_ms', 0):.1f}"
            fp = ""
            rn = ""
            try:
                parsed = json.loads(r.get("agent_response", "").strip())
                fp = parsed.get("failure_point", "").replace('"', '""')
                rn = parsed.get("restart_node", "").replace('"', '""')
            except Exception:
                pass
            f.write(f'{i},"{agent}","{judge}","{r["prompt"]}","{r["variant"]}","{v["scale"]}","{v["domain"]}","{v["mix"]}","{v["noise"]}",{t},{re},{rx},{ro},{rcb},{rexp},{p},{rc},{rd},{lat},"{fp}","{rn}"\n')


def _build_variance_section(md, results):
    """Append within-condition variance section when multiple seeds exist."""
    seeds_seen = {parse_variant(r["variant"]).get("seed", "0") for r in results}
    if len(seeds_seen) <= 1:
        return

    md.append("### Within-Condition Variance (across seeds)")
    md.append("")
    md.append("Multiple graph seeds per cell enable variance estimation.")
    md.append("Each row groups evals by condition (scale\u00d7domain\u00d7mix\u00d7noise), across seeds.")
    md.append("")
    md.append(HDR_VARIANCE)
    md.append(SEP_VARIANCE)

    conditions = OrderedDict()
    for r in results:
        v = parse_variant(r["variant"])
        cond = f"{v['scale']}-{v['domain']}-{v['mix']}-{v['noise']}"
        seed = v.get("seed", "0")
        conditions.setdefault(cond, {}).setdefault(seed, []).append(r)

    for cond, seed_map in sorted(conditions.items()):
        if len(seed_map) < 2:
            continue
        seed_rates = []
        for seed_key, seed_results in sorted(seed_map.items()):
            n = len(seed_results)
            ok = sum(1 for r in seed_results if r.get("task") is True)
            seed_rates.append(ok / n if n > 0 else 0.0)
        mean = sum(seed_rates) / len(seed_rates)
        var = sum((r - mean) ** 2 for r in seed_rates) / len(seed_rates)
        rates_str = ", ".join(f"{r:.0%}" for r in seed_rates)
        append_row(md, [cond, str(len(seed_map)), rates_str, f"{var:.4f}"])

    md.append("")


def _build_token_efficiency(md, results, by_mix):
    """Append token efficiency section if quality metrics are available."""
    has_quality = any(r.get("compression_ratio", 0) > 0 for r in results)
    if not has_quality:
        return

    md.append("## Token Efficiency (kernel-reported)")
    md.append("")
    md.append("Compression ratio = raw_equivalent_tokens / rendered_tokens. >1.0 means the structured graph uses fewer tokens than a flat text dump of the same data.")
    md.append("")
    md.append(HDR_TOKEN)
    md.append(SEP_TOKEN)

    for mix in ["explanatory", "structural", "mixed"]:
        cell = by_mix.get(mix, [])
        if not cell:
            continue
        rendered = [r["rendered_tokens"] for r in cell if r.get("rendered_tokens", 0) > 0]
        raw = [r["raw_equivalent_tokens"] for r in cell if r.get("raw_equivalent_tokens", 0) > 0]
        comp = [r["compression_ratio"] for r in cell if r.get("compression_ratio", 0) > 0]
        cd = [r["causal_density"] for r in cell if "causal_density" in r]
        dc = [r["detail_coverage"] for r in cell if "detail_coverage" in r]
        avg_r = f"{sum(rendered)/len(rendered):.0f}" if rendered else "n/a"
        avg_raw = f"{sum(raw)/len(raw):.0f}" if raw else "n/a"
        avg_c = f"{sum(comp)/len(comp):.2f}x" if comp else "n/a"
        avg_cd = f"{sum(cd)/len(cd):.0%}" if cd else "n/a"
        avg_dc = f"{sum(dc)/len(dc):.0%}" if dc else "n/a"
        append_row(md, [f"**{mix}**", avg_r, avg_raw, f"**{avg_c}**", avg_cd, avg_dc])

    md.append("")


def _build_kernel_metrics_and_export(md, run_dir):
    """Append kernel metrics figure references and data export section."""
    md.append("## Kernel Metrics")
    md.append(fig_ref(run_dir, "11_kernel_token_efficiency.png", "Kernel Token Efficiency"))
    md.append(fig_ref(run_dir, "12_kernel_causal_signal.png", "Causal Signal in Rendered Context"))
    md.append(fig_ref(run_dir, "14_causal_depth_reached.png", "Causal Depth Reached by Agent"))
    md.append(fig_ref(run_dir, "16_chain_depth_vs_task.png", "Chain Depth vs Task Success"))
    md.append(fig_ref(run_dir, "13_response_parsing.png", "Response Parsing Reliability"))
    md.append(fig_ref(run_dir, "06_judge_bias.png", "Judge Bias"))
    md.append("")
    md.append("## Data Export")
    md.append("")
    md.append("CSV with all 720 evaluations: `results.csv`")
    md.append("")


def _build_eval_listing(md, results):
    """Append the line-by-line evaluation listing."""
    md.append("## All Evaluations (line by line)")
    md.append("")
    md.append(HDR_EVALS)
    md.append(SEP_EVALS)

    for i, r in enumerate(results, 1):
        agent, judge = parse_model(r["model"])
        t = tristate_label(r.get("task"))
        re = tristate_label(r.get("restart"))
        p = tristate_label(r.get("reason"))
        lat = f"{r.get('latency_ms', 0):.0f}ms"

        # Parse agent response for failure_point and restart_node
        fp = "?"
        rn = "?"
        try:
            resp = r.get("agent_response", "")
            parsed = json.loads(resp.strip())
            fp = parsed.get("failure_point", "?")[:60]
            rn = parsed.get("restart_node", "?")[:60]
        except Exception:
            fp = "(parse error)"
            rn = "(parse error)"

        rc = tristate_label(r.get("reason_correct"), "OK", "FAIL", "-")
        rd = "LEAK" if r.get("reason_distractor") is True else "-"
        append_row(md, [str(i), agent, judge, r["prompt"], r["variant"], t, re, p, rc, rd, lat, fp, rn])


def generate_report(run_dir, results):
    md = []
    md.append("# E2E Evaluation Matrix Report")
    md.append("")
    md.append(f"**Run**: `{run_dir.name}`")
    md.append(f"**Date**: {run_dir.name[:10]}")
    md.append(f"**Total evaluations**: {len(results)}")
    md.append(fig_ref(run_dir, "00_hypothesis_summary.png", "Hypothesis Validation Summary"))
    md.append("")

    by_cell, by_agent, by_judge, by_prompt, by_mix, by_scale, by_domain, by_noise = _parse_results(results)

    _build_overview(md, results)
    _build_summary_tables(md, run_dir, by_cell, by_agent, by_judge, by_prompt)
    _build_dimension_tables(md, run_dir, by_mix, by_scale, by_domain, by_noise)
    _build_heatmaps(md, run_dir, results, by_agent, by_noise)
    _build_controlled_comparisons(md, results, by_agent, by_judge)
    _build_convergence_table(md, run_dir, by_cell)
    _build_latency_and_cost(md, run_dir, results, by_agent, by_judge)
    _build_restart_diagnostics(md, by_noise)
    _build_statistical_summary(md, run_dir, by_agent, by_mix)
    _export_csv(run_dir, results)
    _build_variance_section(md, results)
    _build_token_efficiency(md, results, by_mix)
    _build_kernel_metrics_and_export(md, run_dir)
    _build_eval_listing(md, results)

    md.append("")
    md.append(f"---\n\nGenerated from `{run_dir.name}` by `scripts/ci/extract-e2e-report.py`")

    return "\n".join(md)


def main():
    if len(sys.argv) > 1:
        run_dir = Path(sys.argv[1])
    else:
        run_dir = find_latest_run()

    print(f"Extracting report from: {run_dir}", file=sys.stderr)

    results = load_results(run_dir)
    if not results:
        print(f"No results found in {run_dir}/results/", file=sys.stderr)
        sys.exit(1)

    print(f"Loaded {len(results)} evaluations", file=sys.stderr)

    report = generate_report(run_dir, results)

    output_path = run_dir / "report-full.md"
    with open(output_path, "w") as f:
        f.write(report)

    print(f"Report written to: {output_path}", file=sys.stderr)
    print(f"  {len(results)} evaluations, {len(report.splitlines())} lines", file=sys.stderr)


if __name__ == "__main__":
    main()
