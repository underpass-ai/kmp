"""The compact serializer shared by the legacy and structured views."""
import json


def compact(value):
    # Same call as surface_acceptance_metrics.measure(): insertion order kept.
    return json.dumps(value, ensure_ascii=False, separators=(',', ':'))
