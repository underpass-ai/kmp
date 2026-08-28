#!/usr/bin/env python3
"""Generate evaluator-compatible predictions from a trained KMP operator."""

from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path
from typing import Any


ACTION_VALIDATOR = "kernel-operator-action-contract-v1"
SCHEMA_MODE = "strict-no-additional-properties"

RELATION_CLASSES = {
    "structural",
    "causal",
    "motivational",
    "procedural",
    "evidential",
    "constraint",
}

WRITER_INTENTS = {
    "record_turn",
    "record_observation",
    "record_decision",
    "record_feedback",
    "record_delta",
}

WRITER_NODE_KINDS = {
    "turn",
    "observation",
    "decision",
    "feedback",
    "semantic_delta",
    "constraint",
    "preference",
    "derived_value",
    "error_path",
    "success_path",
}

CONFIDENCE_VALUES = {"high", "medium", "low", "unknown"}

RELATION_ALIASES = {
    "conflicts": "contradicts",
    "conflicts_with": "contradicts",
    "conflict_with": "contradicts",
    "matches_question_item": "matches_requirement",
}

RELATION_SPECS = {
    "follows": ("anemic", {"procedural"}),
    "answers": ("anemic", {"evidential"}),
    "uses_background": ("anemic", {"evidential"}),
    "depends_on": ("rich", {"causal"}),
    "chosen_because": ("rich", {"causal", "motivational"}),
    "semantic_delta_from": ("rich", {"causal"}),
    "updates_state": ("rich", {"causal"}),
    "supports": ("rich", {"evidential"}),
    "supersedes": ("rich", {"evidential"}),
    "contradicts": ("rich", {"evidential"}),
    "satisfies_constraint": ("rich", {"constraint"}),
    "violates_constraint": ("rich", {"constraint"}),
    "contributes_to": ("rich", {"evidential"}),
    "excluded_from": ("rich", {"constraint"}),
    "checked_against": ("rich", {"constraint"}),
    "derived_from": ("rich", {"evidential"}),
    "confirms_selection": ("rich", {"evidential", "motivational"}),
    "restates": ("rich", {"evidential"}),
    "corrects": ("rich", {"evidential"}),
    "component_of": ("rich", {"evidential"}),
    "total_of": ("rich", {"evidential"}),
    "same_event_as": ("rich", {"evidential"}),
    "same_entity_as": ("rich", {"evidential"}),
    "qualifies_as": ("rich", {"evidential"}),
    "matches_requirement": ("rich", {"constraint"}),
    "contains": ("structural", {"structural"}),
    "member_of": ("structural", {"structural"}),
    "scoped_to": ("structural", {"structural"}),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Predict KMP operator actions.")
    parser.add_argument("--dataset-jsonl", required=True, type=Path)
    parser.add_argument("--model-id", required=True)
    parser.add_argument("--adapter", default=None)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--trust-remote-code", action="store_true")
    parser.add_argument(
        "--torch-dtype",
        choices=["auto", "float16", "bfloat16", "float32"],
        default="auto",
    )
    parser.add_argument(
        "--config-override",
        action="append",
        default=[],
        metavar="KEY=VALUE",
        help="Override a model config attribute before loading the model.",
    )
    parser.add_argument("--limit", type=int, default=None)
    parser.add_argument("--batch-size", type=int, default=8)
    parser.add_argument("--max-new-tokens", type=int, default=350)
    parser.add_argument("--temperature", type=float, default=0.0)
    parser.add_argument(
        "--stop-after-json",
        action="store_true",
        help=(
            "Force EOS after the first complete top-level JSON object is "
            "generated. The parser remains strict and rejects invalid actions."
        ),
    )
    parser.add_argument("--force", action="store_true")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.batch_size < 1:
        raise SystemExit("--batch-size must be >= 1")
    if args.output.exists():
        if not args.force:
            raise SystemExit(f"output already exists: {args.output}; pass --force")
        shutil.rmtree(args.output)
    args.output.mkdir(parents=True)

    try:
        import torch
        from peft import PeftModel
        from transformers import (
            AutoConfig,
            AutoModelForCausalLM,
            AutoTokenizer,
            LogitsProcessorList,
        )
    except ImportError as exc:
        raise SystemExit(
            "Missing inference dependencies. Install torch, transformers, peft, "
            "and accelerate in the inference environment."
        ) from exc

    tokenizer = AutoTokenizer.from_pretrained(
        args.model_id,
        use_fast=True,
        trust_remote_code=args.trust_remote_code,
    )
    if tokenizer.pad_token is None:
        tokenizer.pad_token = tokenizer.eos_token
    tokenizer.padding_side = "left"
    config = AutoConfig.from_pretrained(
        args.model_id,
        trust_remote_code=args.trust_remote_code,
    )
    for key, value in parse_config_overrides(args.config_override).items():
        setattr(config, key, value)
    model = AutoModelForCausalLM.from_pretrained(
        args.model_id,
        config=config,
        torch_dtype=torch_dtype(args.torch_dtype, torch),
        device_map="auto",
        trust_remote_code=args.trust_remote_code,
    )
    if args.adapter:
        model = PeftModel.from_pretrained(model, args.adapter)
    model.config.pad_token_id = tokenizer.pad_token_id
    model.eval()

    rows = read_jsonl(args.dataset_jsonl)
    if args.limit is not None:
        rows = rows[: args.limit]

    predictions_path = args.output / "predictions.jsonl"
    results_path = args.output / "llm_results.jsonl"
    failures_path = args.output / "failures.jsonl"
    predictions = 0
    failures = 0
    failure_reasons: dict[str, int] = {}

    with predictions_path.open("w", encoding="utf-8") as pred_handle, results_path.open(
        "w", encoding="utf-8"
    ) as result_handle, failures_path.open("w", encoding="utf-8") as failure_handle:
        for batch_start in range(0, len(rows), args.batch_size):
            batch = rows[batch_start : batch_start + args.batch_size]
            prompts = [
                tokenizer.apply_chat_template(
                    row["messages"][:-1],
                    tokenize=False,
                    add_generation_prompt=True,
                )
                for row in batch
            ]
            inputs = tokenizer(prompts, return_tensors="pt", padding=True).to(model.device)
            prompt_width = inputs["input_ids"].shape[-1]
            with torch.inference_mode():
                generation_kwargs = {
                    "max_new_tokens": args.max_new_tokens,
                    "do_sample": args.temperature > 0,
                    "eos_token_id": tokenizer.eos_token_id,
                    "pad_token_id": tokenizer.eos_token_id,
                }
                if args.temperature > 0:
                    generation_kwargs["temperature"] = args.temperature
                if args.stop_after_json:
                    generation_kwargs["logits_processor"] = LogitsProcessorList(
                        [
                            StopAfterCompleteJsonObjectProcessor(
                                tokenizer=tokenizer,
                                prompt_width=prompt_width,
                                eos_token_id=tokenizer.eos_token_id,
                            )
                        ]
                    )
                output = model.generate(
                    **inputs,
                    **generation_kwargs,
                )

            for row, generated_output in zip(batch, output, strict=True):
                generated = generated_output[prompt_width:]
                raw = tokenizer.decode(generated, skip_special_tokens=True).strip()
                action, failure_reason = parse_action(raw)
                result = {
                    "step_id": row["step_id"],
                    "raw_response": raw,
                    "action": action,
                    "valid_action": action is not None,
                }
                if failure_reason is not None:
                    result["failure_reason"] = failure_reason
                result_handle.write(
                    json.dumps(result, separators=(",", ":"), sort_keys=True) + "\n"
                )
                if action is None:
                    failures += 1
                    reason = failure_reason or "invalid_action"
                    failure_reasons[reason] = failure_reasons.get(reason, 0) + 1
                    failure_handle.write(
                        json.dumps(
                            {
                                "step_id": row["step_id"],
                                "reason": reason,
                                "raw_response": raw,
                            },
                            separators=(",", ":"),
                            sort_keys=True,
                        )
                        + "\n"
                    )
                    continue
                pred_handle.write(
                    json.dumps(
                        {"step_id": row["step_id"], "action": action},
                        separators=(",", ":"),
                        sort_keys=True,
                    )
                    + "\n"
                )
                predictions += 1

            pred_handle.flush()
            result_handle.flush()
            failure_handle.flush()
            completed = min(batch_start + len(batch), len(rows))
            print(
                json.dumps(
                    {
                        "event": "kernel_operator_predict.progress",
                        "completed": completed,
                        "selected": len(rows),
                        "predictions": predictions,
                        "failures": failures,
                    },
                    separators=(",", ":"),
                    sort_keys=True,
                ),
                file=sys.stderr,
            )

    summary = {
        "predictor": "kernel-operator-sft-predict-v2-batched",
        "action_validator": ACTION_VALIDATOR,
        "schema_mode": SCHEMA_MODE,
        "dataset": str(args.dataset_jsonl),
        "model_id": args.model_id,
        "adapter": args.adapter,
        "selected": len(rows),
        "predictions": predictions,
        "failures": failures,
        "batch_size": args.batch_size,
        "failure_reasons": failure_reasons,
        "max_new_tokens": args.max_new_tokens,
        "stop_after_json": args.stop_after_json,
        "temperature": args.temperature,
    }
    (args.output / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(json.dumps(summary, indent=2, sort_keys=True))


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            if line.strip():
                rows.append(json.loads(line))
    return rows


def parse_config_overrides(overrides: list[str]) -> dict[str, Any]:
    parsed: dict[str, Any] = {}
    for override in overrides:
        if "=" not in override:
            raise SystemExit(f"invalid --config-override {override!r}; expected KEY=VALUE")
        key, raw_value = override.split("=", 1)
        key = key.strip()
        if not key:
            raise SystemExit(f"invalid --config-override {override!r}; empty key")
        parsed[key] = parse_config_value(raw_value.strip())
    return parsed


def parse_config_value(value: str) -> Any:
    lower = value.lower()
    if lower == "true":
        return True
    if lower == "false":
        return False
    if lower == "null":
        return None
    try:
        return int(value)
    except ValueError:
        return value


def torch_dtype(value: str, torch: Any) -> str | Any:
    if value == "auto":
        return "auto"
    return {
        "float16": torch.float16,
        "bfloat16": torch.bfloat16,
        "float32": torch.float32,
    }[value]


class StopAfterCompleteJsonObjectProcessor:
    """Force EOS once each row has produced one complete JSON object."""

    def __init__(self, tokenizer: Any, prompt_width: int, eos_token_id: int) -> None:
        self.tokenizer = tokenizer
        self.prompt_width = prompt_width
        self.eos_token_id = eos_token_id

    def __call__(self, input_ids: Any, scores: Any) -> Any:
        for row_index in range(input_ids.shape[0]):
            generated_ids = input_ids[row_index, self.prompt_width :]
            text = self.tokenizer.decode(generated_ids, skip_special_tokens=True)
            if first_complete_json_object_end(text) is not None:
                scores[row_index].fill_(-float("inf"))
                scores[row_index, self.eos_token_id] = 0
        return scores


def parse_action(raw: str) -> tuple[dict[str, Any] | None, str | None]:
    text = raw.strip()
    if not text:
        return None, "empty_response"
    end = first_complete_json_object_end(text)
    if end is None:
        return None, "incomplete_json"
    prefix = text[: text.find("{")].strip()
    suffix = text[end + 1 :].strip()
    if prefix or suffix:
        return None, "extra_content_after_json"
    try:
        value = json.loads(text)
    except json.JSONDecodeError:
        return None, "invalid_json"
    if not isinstance(value, dict):
        return None, "json_not_object"
    if set(value.keys()) != {"action"}:
        return None, "missing_or_extra_top_level_fields"
    action = value["action"]
    if not isinstance(action, dict):
        return None, "action_not_object"
    shape_error = validate_action_shape(action)
    if shape_error is not None:
        return None, shape_error
    return action, None


def first_complete_json_object_end(text: str) -> int | None:
    start = text.find("{")
    if start < 0:
        return None
    if text[:start].strip():
        return None

    depth = 0
    in_string = False
    escaped = False
    for index, char in enumerate(text[start:], start=start):
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue

        if char == '"':
            in_string = True
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return index
            if depth < 0:
                return None

    return None


def validate_action_shape(action: dict[str, Any]) -> str | None:
    action_type = action.get("type")
    if action_type == "tool_call":
        key_error = exact_keys(action, {"type", "tool", "arguments"}, set(), "action")
        if key_error is not None:
            return key_error
        tool = action.get("tool")
        if not isinstance(tool, str) or not tool:
            return "missing_tool"
        arguments = action.get("arguments")
        if not isinstance(arguments, dict):
            return "missing_arguments"
        argument_error = validate_tool_arguments(tool, arguments)
        if argument_error is not None:
            return argument_error
        if not is_bounded_tool_call(tool, arguments):
            return f"unbounded_or_invalid_tool_call:{tool}"
        return None

    if action_type == "stop":
        key_error = exact_keys(
            action,
            {"type", "answer_policy", "final_refs", "reason"},
            set(),
            "action",
        )
        if key_error is not None:
            return key_error
        answer_policy_error = validate_answer_policy(action.get("answer_policy"))
        if answer_policy_error is not None:
            return answer_policy_error
        reason = action.get("reason")
        if not isinstance(reason, str) or not reason:
            return "missing_stop_reason"
        final_refs = action.get("final_refs")
        if not isinstance(final_refs, list) or not all(
            isinstance(ref, str) for ref in final_refs
        ):
            return "invalid_stop_final_refs"
        return None

    if action_type is None:
        return "missing_action_type"
    return "unsupported_action_type"


def validate_tool_arguments(tool: str, arguments: dict[str, Any]) -> str | None:
    if tool == "kmp_wake":
        key_error = exact_keys(
            arguments,
            {"about"},
            {"role", "intent", "dimensions", "depth", "budget"},
            "action.arguments",
        )
        if key_error is not None:
            return key_error
        error = require_non_empty_string(arguments, "about", "action.arguments")
        if error is not None:
            return error
        for field in ("role", "intent"):
            error = validate_optional_non_empty_string(arguments, field, "action.arguments")
            if error is not None:
                return error
        if "dimensions" in arguments:
            error = validate_dimensions(arguments["dimensions"], "action.arguments.dimensions")
            if error is not None:
                return error
        error = validate_optional_positive_int(arguments, "depth", "action.arguments")
        if error is not None:
            return error
        if "budget" in arguments:
            return validate_budget(arguments["budget"], "action.arguments.budget")
        return None

    if tool == "kmp_ask":
        key_error = exact_keys(
            arguments,
            {"about", "answer_policy", "dimensions", "question"},
            {"budget", "depth"},
            "action.arguments",
        )
        if key_error is not None:
            return key_error
        for field in ("about", "question"):
            error = require_non_empty_string(arguments, field, "action.arguments")
            if error is not None:
                return error
        error = validate_answer_policy(arguments.get("answer_policy"))
        if error is not None:
            return error
        error = validate_dimensions(arguments.get("dimensions"), "action.arguments.dimensions")
        if error is not None:
            return error
        if "budget" in arguments:
            error = validate_budget(arguments["budget"], "action.arguments.budget")
            if error is not None:
                return error
        return validate_optional_positive_int(arguments, "depth", "action.arguments")

    if tool in {"kmp_near", "kmp_goto", "kmp_rewind", "kmp_forward"}:
        cursor_key = {
            "kmp_near": "around",
            "kmp_goto": "at",
            "kmp_rewind": "from",
            "kmp_forward": "from",
        }[tool]
        return validate_temporal_arguments(arguments, cursor_key)

    if tool == "kmp_trace":
        key_error = exact_keys(
            arguments,
            {"about", "from", "to", "budget"},
            {"goal", "role", "page"},
            "action.arguments",
        )
        if key_error is not None:
            return key_error
        for field in ("about", "from", "to"):
            error = require_non_empty_string(arguments, field, "action.arguments")
            if error is not None:
                return error
        error = validate_budget(arguments.get("budget"), "action.arguments.budget")
        if error is not None:
            return error
        for field in ("goal", "role"):
            error = validate_optional_non_empty_string(arguments, field, "action.arguments")
            if error is not None:
                return error
        if "page" in arguments:
            return validate_page(arguments["page"], "action.arguments.page")
        return None

    if tool == "kmp_inspect":
        key_error = exact_keys(
            arguments, {"about", "ref", "include"}, set(), "action.arguments"
        )
        if key_error is not None:
            return key_error
        for field in ("about", "ref"):
            error = require_non_empty_string(arguments, field, "action.arguments")
            if error is not None:
                return error
        return validate_inspect_include(
            arguments.get("include"), "action.arguments.include"
        )

    if tool == "kmp_write_memory":
        return validate_write_memory_arguments(arguments)

    if tool == "kmp_ingest":
        return validate_ingest_arguments(arguments)

    return f"unsupported_tool:{tool}"


def validate_write_memory_arguments(arguments: dict[str, Any]) -> str | None:
    key_error = exact_keys(
        arguments,
        {
            "about",
            "intent",
            "actor",
            "observed_at",
            "scope",
            "current",
            "connect_to",
            "read_context",
            "idempotency_key",
            "options",
        },
        {"semantic_delta", "source_kind"},
        "action.arguments",
    )
    if key_error is not None:
        return key_error
    for field in ("about", "actor", "observed_at", "idempotency_key"):
        error = require_non_empty_string(arguments, field, "action.arguments")
        if error is not None:
            return error
    intent = arguments.get("intent")
    if not isinstance(intent, str) or intent not in WRITER_INTENTS:
        return "action.arguments.intent_unsupported"
    error = validate_write_scope(arguments.get("scope"), "action.arguments.scope")
    if error is not None:
        return error
    error, current_ref = validate_write_current(
        arguments.get("current"), "action.arguments.current"
    )
    if error is not None:
        return error
    semantic_delta_ref = None
    if "semantic_delta" in arguments:
        error, semantic_delta_ref = validate_semantic_delta(
            arguments.get("semantic_delta"), "action.arguments.semantic_delta"
        )
        if error is not None:
            return error
    error, observed_read_refs = read_context_refs(
        arguments.get("read_context"), "action.arguments.read_context"
    )
    if error is not None:
        return error
    local_refs = [
        ref for ref in (current_ref, semantic_delta_ref) if isinstance(ref, str) and ref
    ]
    error = validate_connect_to(
        arguments.get("connect_to"),
        "action.arguments.connect_to",
        local_refs,
        observed_read_refs,
    )
    if error is not None:
        return error
    error = validate_write_options(arguments.get("options"), "action.arguments.options")
    if error is not None:
        return error
    return validate_optional_non_empty_string(
        arguments, "source_kind", "action.arguments"
    )


def validate_ingest_arguments(arguments: dict[str, Any]) -> str | None:
    key_error = exact_keys(
        arguments,
        {"about", "memory", "provenance", "idempotency_key", "dry_run"},
        set(),
        "action.arguments",
    )
    if key_error is not None:
        return key_error
    for field in ("about", "idempotency_key"):
        error = require_non_empty_string(arguments, field, "action.arguments")
        if error is not None:
            return error
    error = validate_ingest_memory(
        arguments.get("memory"), "action.arguments.memory"
    )
    if error is not None:
        return error
    error = validate_ingest_provenance(
        arguments.get("provenance"), "action.arguments.provenance"
    )
    if error is not None:
        return error
    return require_bool(arguments, "dry_run", "action.arguments")


def validate_write_scope(value: Any, context: str) -> str | None:
    if not isinstance(value, dict):
        return f"{context}_not_object"
    key_error = exact_keys(value, {"process"}, {"task", "episode"}, context)
    if key_error is not None:
        return key_error
    error = require_non_empty_string(value, "process", context)
    if error is not None:
        return error
    for field in ("task", "episode"):
        error = validate_optional_non_empty_string(value, field, context)
        if error is not None:
            return error
    return None


def validate_write_current(value: Any, context: str) -> tuple[str | None, str | None]:
    if not isinstance(value, dict):
        return f"{context}_not_object", None
    key_error = exact_keys(value, {"kind", "summary", "evidence"}, {"ref"}, context)
    if key_error is not None:
        return key_error, None
    kind = value.get("kind")
    if not isinstance(kind, str) or kind not in WRITER_NODE_KINDS:
        return f"{context}.kind_unsupported", None
    for field in ("summary", "evidence"):
        error = require_non_empty_string(value, field, context)
        if error is not None:
            return error, None
    error = validate_optional_non_empty_string(value, "ref", context)
    if error is not None:
        return error, None
    return None, value.get("ref")


def validate_semantic_delta(value: Any, context: str) -> tuple[str | None, str | None]:
    if not isinstance(value, dict):
        return f"{context}_not_object", None
    key_error = exact_keys(
        value, {"from", "to", "why", "evidence"}, {"ref"}, context
    )
    if key_error is not None:
        return key_error, None
    for field in ("from", "to", "why", "evidence"):
        error = require_non_empty_string(value, field, context)
        if error is not None:
            return error, None
    error = validate_optional_non_empty_string(value, "ref", context)
    if error is not None:
        return error, None
    return None, value.get("ref")


def read_context_refs(value: Any, context: str) -> tuple[str | None, list[str]]:
    if not isinstance(value, dict):
        return f"{context}_not_object", []
    key_error = exact_keys(
        value,
        set(),
        {"inspected_refs", "temporal_refs", "wake_refs", "ask_refs", "trace_paths"},
        context,
    )
    if key_error is not None:
        return key_error, []
    refs: list[str] = []
    for field in ("inspected_refs", "temporal_refs", "wake_refs", "ask_refs"):
        if field not in value:
            continue
        error = validate_string_array(value[field], f"{context}.{field}")
        if error is not None:
            return error, []
        refs.extend(value[field])
    if "trace_paths" in value:
        paths = value["trace_paths"]
        if not isinstance(paths, list):
            return f"{context}.trace_paths_not_array", []
        for index, path in enumerate(paths):
            path_context = f"{context}.trace_paths[{index}]"
            if not isinstance(path, dict):
                return f"{path_context}_not_object", []
            key_error = exact_keys(path, {"from", "to"}, {"refs"}, path_context)
            if key_error is not None:
                return key_error, []
            for field in ("from", "to"):
                error = require_non_empty_string(path, field, path_context)
                if error is not None:
                    return error, []
                refs.append(path[field])
            if "refs" in path:
                error = validate_string_array(path["refs"], f"{path_context}.refs")
                if error is not None:
                    return error, []
                refs.extend(path["refs"])
    return None, refs


def validate_connect_to(
    value: Any,
    context: str,
    local_refs: list[str],
    read_context_refs_value: list[str],
) -> str | None:
    if not isinstance(value, list):
        return f"{context}_not_array"
    if not value:
        return f"{context}_empty"
    for index, link in enumerate(value):
        link_context = f"{context}[{index}]"
        if not isinstance(link, dict):
            return f"{link_context}_not_object"
        key_error = exact_keys(
            link,
            {"ref", "rel", "class", "why", "evidence"},
            {"confidence"},
            link_context,
        )
        if key_error is not None:
            return key_error
        for field in ("ref", "rel", "class"):
            error = require_non_empty_string(link, field, link_context)
            if error is not None:
                return error
        target_ref = link["ref"]
        relation = normalize_relation(link["rel"])
        if relation not in RELATION_SPECS:
            return f"{link_context}.rel_outside_writer_vocabulary"
        semantic_class = link["class"]
        if semantic_class not in RELATION_CLASSES:
            return f"{link_context}.class_invalid"
        quality, allowed_classes = RELATION_SPECS[relation]
        if semantic_class not in allowed_classes:
            return f"{link_context}.class_not_allowed_for_relation:{relation}"
        if semantic_class != "structural":
            for field in ("why", "evidence"):
                error = require_non_empty_string(link, field, link_context)
                if error is not None:
                    return error
        if "confidence" in link:
            confidence = link.get("confidence")
            if not isinstance(confidence, str) or confidence not in CONFIDENCE_VALUES:
                return f"{link_context}.confidence_unsupported"
        target_is_local = target_ref in local_refs
        target_was_read = target_ref in read_context_refs_value
        if quality == "rich" and not target_is_local and not target_was_read:
            return f"{link_context}.ref_missing_read_context_proof"
    return None


def validate_write_options(value: Any, context: str) -> str | None:
    if not isinstance(value, dict):
        return f"{context}_not_object"
    key_error = exact_keys(value, {"dry_run", "strict"}, {"sequence"}, context)
    if key_error is not None:
        return key_error
    error = require_bool(value, "dry_run", context)
    if error is not None:
        return error
    error = require_bool(value, "strict", context)
    if error is not None:
        return error
    if value.get("strict") is not True:
        return f"{context}.strict_must_be_true"
    return validate_optional_positive_int(value, "sequence", context)


def validate_ingest_memory(value: Any, context: str) -> str | None:
    if not isinstance(value, dict):
        return f"{context}_not_object"
    key_error = exact_keys(
        value, {"dimensions", "entries", "relations", "evidence"}, set(), context
    )
    if key_error is not None:
        return key_error
    validators = (
        validate_dimensions_payload,
        validate_entries_payload,
        validate_relations_payload,
        validate_evidence_payload,
    )
    for field, validator in zip(
        ("dimensions", "entries", "relations", "evidence"), validators, strict=True
    ):
        error = validator(value.get(field), context)
        if error is not None:
            return error
    return None


def validate_dimensions_payload(value: Any, context: str) -> str | None:
    if not isinstance(value, list):
        return f"{context}.dimensions_not_array"
    if not value:
        return f"{context}.dimensions_empty"
    for index, dimension in enumerate(value):
        dimension_context = f"{context}.dimensions[{index}]"
        if not isinstance(dimension, dict):
            return f"{dimension_context}_not_object"
        key_error = exact_keys(dimension, {"id", "kind"}, {"title"}, dimension_context)
        if key_error is not None:
            return key_error
        for field in ("id", "kind"):
            error = require_non_empty_string(dimension, field, dimension_context)
            if error is not None:
                return error
        error = validate_optional_non_empty_string(dimension, "title", dimension_context)
        if error is not None:
            return error
    return None


def validate_entries_payload(value: Any, context: str) -> str | None:
    if not isinstance(value, list):
        return f"{context}.entries_not_array"
    if not value:
        return f"{context}.entries_empty"
    for index, entry in enumerate(value):
        entry_context = f"{context}.entries[{index}]"
        if not isinstance(entry, dict):
            return f"{entry_context}_not_object"
        key_error = exact_keys(
            entry, {"id", "kind", "text"}, {"coordinates", "metadata"}, entry_context
        )
        if key_error is not None:
            return key_error
        for field in ("id", "kind", "text"):
            error = require_non_empty_string(entry, field, entry_context)
            if error is not None:
                return error
        if "coordinates" in entry:
            error = validate_coordinates(entry["coordinates"], f"{entry_context}.coordinates")
            if error is not None:
                return error
    return None


def validate_coordinates(value: Any, context: str) -> str | None:
    if not isinstance(value, list):
        return f"{context}_not_array"
    for index, coordinate in enumerate(value):
        coordinate_context = f"{context}[{index}]"
        if not isinstance(coordinate, dict):
            return f"{coordinate_context}_not_object"
        key_error = exact_keys(
            coordinate,
            {"dimension", "scope_id"},
            {"sequence", "observed_at", "occurred_at", "valid_from"},
            coordinate_context,
        )
        if key_error is not None:
            return key_error
        for field in ("dimension", "scope_id"):
            error = require_non_empty_string(coordinate, field, coordinate_context)
            if error is not None:
                return error
        error = validate_optional_positive_int(coordinate, "sequence", coordinate_context)
        if error is not None:
            return error
        for field in ("observed_at", "occurred_at", "valid_from"):
            error = validate_optional_non_empty_string(coordinate, field, coordinate_context)
            if error is not None:
                return error
    return None


def validate_relations_payload(value: Any, context: str) -> str | None:
    if not isinstance(value, list):
        return f"{context}.relations_not_array"
    for index, relation in enumerate(value):
        relation_context = f"{context}.relations[{index}]"
        if not isinstance(relation, dict):
            return f"{relation_context}_not_object"
        key_error = exact_keys(
            relation,
            {"from", "to", "rel", "class", "why", "evidence"},
            {"confidence", "sequence"},
            relation_context,
        )
        if key_error is not None:
            return key_error
        for field in ("from", "to", "rel", "class", "why", "evidence"):
            error = require_non_empty_string(relation, field, relation_context)
            if error is not None:
                return error
        if not normalize_relation(relation["rel"]):
            return f"{relation_context}.rel_invalid"
        semantic_class = relation["class"]
        if semantic_class not in RELATION_CLASSES:
            return f"{relation_context}.class_invalid"
        if "confidence" in relation:
            confidence = relation.get("confidence")
            if not isinstance(confidence, str) or confidence not in CONFIDENCE_VALUES:
                return f"{relation_context}.confidence_unsupported"
        error = validate_optional_positive_int(relation, "sequence", relation_context)
        if error is not None:
            return error
    return None


def validate_evidence_payload(value: Any, context: str) -> str | None:
    if not isinstance(value, list):
        return f"{context}.evidence_not_array"
    for index, evidence in enumerate(value):
        evidence_context = f"{context}.evidence[{index}]"
        if not isinstance(evidence, dict):
            return f"{evidence_context}_not_object"
        key_error = exact_keys(
            evidence, {"id", "supports", "text"}, {"source", "time"}, evidence_context
        )
        if key_error is not None:
            return key_error
        error = require_non_empty_string(evidence, "id", evidence_context)
        if error is not None:
            return error
        error = validate_string_array(evidence.get("supports"), f"{evidence_context}.supports")
        if error is not None:
            return error
        error = require_non_empty_string(evidence, "text", evidence_context)
        if error is not None:
            return error
        for field in ("source", "time"):
            error = validate_optional_non_empty_string(evidence, field, evidence_context)
            if error is not None:
                return error
    return None


def validate_ingest_provenance(value: Any, context: str) -> str | None:
    if not isinstance(value, dict):
        return f"{context}_not_object"
    key_error = exact_keys(
        value,
        {"source_kind", "source_agent", "observed_at", "correlation_id", "causation_id"},
        set(),
        context,
    )
    if key_error is not None:
        return key_error
    for field in (
        "source_kind",
        "source_agent",
        "observed_at",
        "correlation_id",
        "causation_id",
    ):
        error = require_non_empty_string(value, field, context)
        if error is not None:
            return error
    return None


def normalize_relation(value: str) -> str:
    normalized = value.strip().lower().replace("-", "_").replace(" ", "_")
    return RELATION_ALIASES.get(normalized, normalized)


def validate_temporal_arguments(arguments: dict[str, Any], cursor_key: str) -> str | None:
    key_error = exact_keys(
        arguments,
        {
            "about",
            cursor_key,
            "dimensions",
            "include",
            "limit",
            "budget",
            "window",
        },
        {"depth"},
        "action.arguments",
    )
    if key_error is not None:
        return key_error
    error = require_non_empty_string(arguments, "about", "action.arguments")
    if error is not None:
        return error
    error = validate_temporal_cursor(arguments.get(cursor_key), f"action.arguments.{cursor_key}")
    if error is not None:
        return error
    error = validate_dimensions(arguments.get("dimensions"), "action.arguments.dimensions")
    if error is not None:
        return error
    error = validate_temporal_include(arguments.get("include"), "action.arguments.include")
    if error is not None:
        return error
    error = validate_limit(arguments.get("limit"), "action.arguments.limit")
    if error is not None:
        return error
    error = validate_budget(arguments.get("budget"), "action.arguments.budget")
    if error is not None:
        return error
    error = validate_window(arguments.get("window"), "action.arguments.window")
    if error is not None:
        return error
    return validate_optional_positive_int(arguments, "depth", "action.arguments")


def validate_dimensions(value: Any, context: str) -> str | None:
    if not isinstance(value, dict):
        return f"{context}_not_object"
    key_error = exact_keys(
        value,
        {"mode", "scope"},
        {"include", "exclude", "scope_ids", "abouts"},
        context,
    )
    if key_error is not None:
        return key_error
    mode = value.get("mode")
    if mode not in {"all", "only", "except"}:
        return f"{context}.mode_unsupported"
    scope = value.get("scope")
    if scope not in {"current_about", "abouts", "all_abouts"}:
        return f"{context}.scope_unsupported"
    for field in ("include", "exclude", "scope_ids", "abouts"):
        if field in value:
            error = validate_string_array(value[field], f"{context}.{field}")
            if error is not None:
                return error
    include_count = len(value.get("include", []))
    exclude_count = len(value.get("exclude", []))
    abouts_count = len(value.get("abouts", []))
    if mode == "all" and (include_count > 0 or exclude_count > 0):
        return f"{context}.mode_all_with_include_or_exclude"
    if mode == "only" and include_count == 0:
        return f"{context}.mode_only_requires_include"
    if mode == "only" and exclude_count > 0:
        return f"{context}.mode_only_with_exclude"
    if mode == "except" and exclude_count == 0:
        return f"{context}.mode_except_requires_exclude"
    if mode == "except" and include_count > 0:
        return f"{context}.mode_except_with_include"
    if scope == "current_about" and abouts_count > 0:
        return f"{context}.scope_current_about_with_abouts"
    if scope == "abouts" and abouts_count == 0:
        return f"{context}.scope_abouts_requires_abouts"
    if scope == "all_abouts" and abouts_count > 0:
        return f"{context}.scope_all_abouts_with_abouts"
    return None


def validate_temporal_cursor(value: Any, context: str) -> str | None:
    if not isinstance(value, dict):
        return f"{context}_not_object"
    key_error = exact_keys(value, set(), {"ref", "time", "sequence"}, context)
    if key_error is not None:
        return key_error
    selected = [field for field in ("ref", "time", "sequence") if field in value]
    if len(selected) != 1:
        return f"{context}_must_set_exactly_one_cursor"
    field = selected[0]
    if field in {"ref", "time"}:
        return require_non_empty_string(value, field, context)
    return require_positive_int(value, "sequence", context)


def validate_temporal_include(value: Any, context: str) -> str | None:
    if not isinstance(value, dict):
        return f"{context}_not_object"
    key_error = exact_keys(value, {"evidence", "raw_refs", "relations"}, set(), context)
    if key_error is not None:
        return key_error
    for field in ("evidence", "raw_refs", "relations"):
        error = require_bool(value, field, context)
        if error is not None:
            return error
    return None


def validate_inspect_include(value: Any, context: str) -> str | None:
    if not isinstance(value, dict):
        return f"{context}_not_object"
    key_error = exact_keys(
        value, {"details", "incoming", "outgoing", "raw"}, set(), context
    )
    if key_error is not None:
        return key_error
    for field in ("details", "incoming", "outgoing", "raw"):
        error = require_bool(value, field, context)
        if error is not None:
            return error
    if value["raw"]:
        return f"{context}.raw_must_be_false"
    return None


def validate_limit(value: Any, context: str) -> str | None:
    if not isinstance(value, dict):
        return f"{context}_not_object"
    key_error = exact_keys(value, {"entries", "tokens"}, set(), context)
    if key_error is not None:
        return key_error
    for field in ("entries", "tokens"):
        error = require_positive_int(value, field, context)
        if error is not None:
            return error
    return None


def validate_budget(value: Any, context: str) -> str | None:
    if not isinstance(value, dict):
        return f"{context}_not_object"
    key_error = exact_keys(value, set(), {"tokens", "depth", "detail"}, context)
    if key_error is not None:
        return key_error
    if not value:
        return f"{context}_empty"
    for field in ("tokens", "depth"):
        error = validate_optional_positive_int(value, field, context)
        if error is not None:
            return error
    detail = value.get("detail")
    if detail is not None and detail not in {"compact", "balanced", "full"}:
        return f"{context}.detail_unsupported"
    return None


def validate_window(value: Any, context: str) -> str | None:
    if not isinstance(value, dict):
        return f"{context}_not_object"
    key_error = exact_keys(value, {"before_entries", "after_entries"}, set(), context)
    if key_error is not None:
        return key_error
    for field in ("before_entries", "after_entries"):
        error = require_nonnegative_int(value, field, context)
        if error is not None:
            return error
    return None


def validate_page(value: Any, context: str) -> str | None:
    if not isinstance(value, dict):
        return f"{context}_not_object"
    key_error = exact_keys(value, set(), {"entries", "cursor"}, context)
    if key_error is not None:
        return key_error
    error = validate_optional_positive_int(value, "entries", context)
    if error is not None:
        return error
    return validate_optional_non_empty_string(value, "cursor", context)


def validate_answer_policy(value: Any) -> str | None:
    if value not in {"evidence_or_unknown", "show_conflicts", "best_effort"}:
        return "unsupported_answer_policy"
    return None


def is_bounded_tool_call(tool: str, arguments: dict[str, Any]) -> bool:
    if tool == "kmp_wake":
        return (
            path_non_empty_string(arguments, ("about",))
            and optional_limit(arguments, ("budget", "tokens"), 16_000)
            and optional_limit(arguments, ("budget", "depth"), 8)
            and optional_limit(arguments, ("depth",), 8)
        )
    if tool == "kmp_near":
        return (
            positive_limit(arguments, ("limit", "entries"), 64)
            and positive_limit(arguments, ("limit", "tokens"), 16_000)
            and optional_limit(arguments, ("budget", "tokens"), 16_000)
            and optional_limit(arguments, ("budget", "depth"), 8)
            and optional_limit(arguments, ("window", "before_entries"), 64)
            and optional_limit(arguments, ("window", "after_entries"), 64)
            and path_cursor(arguments, ("around",)) is not None
        )
    if tool == "kmp_trace":
        return (
            path_non_empty_string(arguments, ("about",))
            and path_string(arguments, ("from",)) is not None
            and path_string(arguments, ("to",)) is not None
            and positive_limit(arguments, ("budget", "tokens"), 16_000)
            and optional_limit(arguments, ("budget", "depth"), 8)
            and optional_limit(arguments, ("page", "entries"), 256)
        )
    if tool == "kmp_inspect":
        return (
            path_non_empty_string(arguments, ("about",))
            and path_string(arguments, ("ref",)) is not None
            and arguments.get("include", {}).get("raw") is False
        )
    if tool in {"kmp_goto", "kmp_rewind", "kmp_forward"}:
        cursor_key = "at" if tool == "kmp_goto" else "from"
        return (
            path_cursor(arguments, (cursor_key,)) is not None
            and optional_limit(arguments, ("limit", "entries"), 64)
            and optional_limit(arguments, ("limit", "tokens"), 16_000)
            and optional_limit(arguments, ("budget", "tokens"), 16_000)
        )
    if tool == "kmp_ask":
        return optional_limit(arguments, ("budget", "tokens"), 16_000)
    if tool == "kmp_write_memory":
        connect_to = arguments.get("connect_to")
        return (
            validate_write_memory_arguments(arguments) is None
            and arguments.get("options", {}).get("strict") is True
            and isinstance(arguments.get("options", {}).get("dry_run"), bool)
            and optional_limit(arguments, ("options", "sequence"), 2**32 - 1)
            and isinstance(connect_to, list)
            and 0 < len(connect_to) <= 32
        )
    if tool == "kmp_ingest":
        memory = arguments.get("memory", {})
        dimensions = memory.get("dimensions") if isinstance(memory, dict) else None
        entries = memory.get("entries") if isinstance(memory, dict) else None
        relations = memory.get("relations") if isinstance(memory, dict) else None
        evidence = memory.get("evidence") if isinstance(memory, dict) else None
        return (
            validate_ingest_arguments(arguments) is None
            and isinstance(arguments.get("dry_run"), bool)
            and isinstance(dimensions, list)
            and 0 < len(dimensions) <= 64
            and isinstance(entries, list)
            and 0 < len(entries) <= 256
            and isinstance(relations, list)
            and len(relations) <= 512
            and isinstance(evidence, list)
            and len(evidence) <= 512
        )
    return False


def exact_keys(
    value: dict[str, Any],
    required: set[str],
    optional: set[str],
    context: str,
) -> str | None:
    missing = sorted(required - set(value.keys()))
    if missing:
        return f"{context}_missing_required:{','.join(missing)}"
    unexpected = sorted(set(value.keys()) - required - optional)
    if unexpected:
        return f"{context}_unexpected:{','.join(unexpected)}"
    return None


def require_non_empty_string(value: dict[str, Any], field: str, context: str) -> str | None:
    actual = value.get(field)
    if not isinstance(actual, str) or not actual:
        return f"{context}.{field}_missing_or_empty"
    return None


def validate_optional_non_empty_string(
    value: dict[str, Any], field: str, context: str
) -> str | None:
    if field not in value:
        return None
    return require_non_empty_string(value, field, context)


def require_bool(value: dict[str, Any], field: str, context: str) -> str | None:
    if not isinstance(value.get(field), bool):
        return f"{context}.{field}_not_bool"
    return None


def require_positive_int(value: dict[str, Any], field: str, context: str) -> str | None:
    actual = value.get(field)
    if not isinstance(actual, int) or isinstance(actual, bool) or actual <= 0:
        return f"{context}.{field}_not_positive_int"
    return None


def require_nonnegative_int(value: dict[str, Any], field: str, context: str) -> str | None:
    actual = value.get(field)
    if not isinstance(actual, int) or isinstance(actual, bool) or actual < 0:
        return f"{context}.{field}_not_nonnegative_int"
    return None


def validate_optional_positive_int(
    value: dict[str, Any], field: str, context: str
) -> str | None:
    if field not in value:
        return None
    return require_positive_int(value, field, context)


def validate_string_array(value: Any, context: str) -> str | None:
    if not isinstance(value, list) or not all(
        isinstance(item, str) and item for item in value
    ):
        return f"{context}_not_string_array"
    return None


def positive_limit(value: dict[str, Any], path: tuple[str, ...], maximum: int) -> bool:
    actual = path_int(value, path)
    return actual is not None and 0 < actual <= maximum


def optional_limit(value: dict[str, Any], path: tuple[str, ...], maximum: int) -> bool:
    actual = path_int(value, path)
    return actual is None or actual <= maximum


def path_int(value: dict[str, Any], path: tuple[str, ...]) -> int | None:
    current: Any = value
    for key in path:
        if not isinstance(current, dict):
            return None
        current = current.get(key)
    if isinstance(current, bool) or not isinstance(current, int):
        return None
    return current


def path_string(value: dict[str, Any], path: tuple[str, ...]) -> str | None:
    current: Any = value
    for key in path:
        if not isinstance(current, dict):
            return None
        current = current.get(key)
    return current if isinstance(current, str) and current else None


def path_non_empty_string(value: dict[str, Any], path: tuple[str, ...]) -> bool:
    return path_string(value, path) is not None


def path_cursor(value: dict[str, Any], path: tuple[str, ...]) -> tuple[str, Any] | None:
    current: Any = value
    for key in path:
        if not isinstance(current, dict):
            return None
        current = current.get(key)
    if not isinstance(current, dict):
        return None
    selected = [(key, current[key]) for key in ("ref", "time", "sequence") if key in current]
    if len(selected) != 1:
        return None
    key, actual = selected[0]
    if key in {"ref", "time"}:
        return (key, actual) if isinstance(actual, str) and actual else None
    if isinstance(actual, bool) or not isinstance(actual, int) or actual <= 0:
        return None
    return key, actual


if __name__ == "__main__":
    main()
