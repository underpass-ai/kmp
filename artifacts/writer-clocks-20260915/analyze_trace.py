#!/usr/bin/env python3
"""Measure a complete writer trace without changing its closed store."""
import argparse
from collections import Counter
import json
import math
from pathlib import Path
import statistics

import tiktoken


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[max(0, math.ceil(len(ordered) * fraction) - 1)]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("trace", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    rows = [json.loads(line) for line in args.trace.read_text().splitlines()]
    calls = [row for row in rows if "request" in row]
    encoder = tiktoken.get_encoding("o200k_base")
    methods = []
    statuses = Counter()
    rejected = []
    unknown = []
    for call in calls:
        request = call["request"]
        name = request["method"]
        if name == "tools/call":
            name = request["params"]["name"]
        methods.append(name)
        structured = call["response"].get("result", {}).get("structuredContent", {})
        if structured.get("status"):
            statuses[structured["status"]] += 1
        if call["response"].get("result", {}).get("isError"):
            rejected.append(request["id"])
        if structured.get("answer") == "UNKNOWN" or structured.get("summary", "").startswith("UNKNOWN"):
            unknown.append(request["id"])
    latencies = [call["latency_ms"] for call in calls]
    request_tokens = sum(len(encoder.encode(
        json.dumps(call["request"], ensure_ascii=False, separators=(",", ":")))) for call in calls)
    response_tokens = sum(len(encoder.encode(
        json.dumps(call["response"], ensure_ascii=False, separators=(",", ":")))) for call in calls)
    preparation = [call for call in calls if call["request"]["id"] <= 3]
    interactive = [call for call in calls if call["request"]["id"] > 3]
    emitted = [call for call in calls if call["request"]["id"] != 3]
    def totals(selected):
        return {
            "calls": len(selected),
            "request_bytes": sum(call["request_bytes"] for call in selected),
            "response_bytes": sum(call["response_bytes"] for call in selected),
            "request_tokens": sum(len(encoder.encode(json.dumps(
                call["request"], ensure_ascii=False, separators=(",", ":")))) for call in selected),
            "response_tokens": sum(len(encoder.encode(json.dumps(
                call["response"], ensure_ascii=False, separators=(",", ":")))) for call in selected),
        }
    result = {
        "trace": str(args.trace),
        "tokenizer": f"tiktoken {tiktoken.__version__} o200k_base",
        "calls": len(calls),
        "calls_by_name": dict(sorted(Counter(methods).items())),
        "request_bytes": sum(call["request_bytes"] for call in calls),
        "response_bytes": sum(call["response_bytes"] for call in calls),
        "request_tokens": request_tokens,
        "response_tokens": response_tokens,
        "native_request_response_tokens": request_tokens + response_tokens,
        "preparation_initialize_list_and_hidden_seed": totals(preparation),
        "writer_interactive_native_calls": totals(interactive),
        "driver_emitted_responses": {
            "responses": len(emitted),
            "bytes": sum(call["response_bytes"] for call in emitted),
            "tokens": sum(len(encoder.encode(json.dumps(
                call["response"], ensure_ascii=False, separators=(",", ":")))) for call in emitted),
            "scope": "Responses printed by the driver except the hidden seed response; not measured host context."
        },
        "latency_ms": {
            "sum": sum(latencies),
            "mean": statistics.mean(latencies),
            "median": statistics.median(latencies),
            "p95_nearest_rank": percentile(latencies, 0.95),
            "maximum": max(latencies),
        },
        "write_statuses": dict(sorted(statuses.items())),
        "error_response_ids": rejected,
        "unknown_response_ids": unknown,
        "model_calls_by_driver": rows[0]["model_calls_by_driver"],
    }
    rendered = json.dumps(result, indent=2, ensure_ascii=False) + "\n"
    if args.output:
        args.output.write_text(rendered)
    else:
        print(rendered, end="")


if __name__ == "__main__":
    main()
