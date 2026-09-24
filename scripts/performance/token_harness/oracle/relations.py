"""Kernel relation vocabulary (crates/kmp-domain/src/value_objects/relation_type.rs).

A next action that names one of these as its verb restates a stored link; the
graph records no action type, so it is not a pending task.
"""
import re

RELATION_TYPES = frozenset((
    'answers', 'authorizes', 'checked_against', 'chosen_because', 'component_of',
    'confirms_selection', 'contains', 'contains_entry', 'contradicts', 'contributes_to',
    'corrects', 'depends_on', 'derived_from', 'excluded_from', 'follows', 'has_dimension',
    'has_evidence', 'matches_requirement', 'member_of', 'qualifies_as', 'records', 'restates',
    'same_entity_as', 'same_event_as', 'satisfies_constraint', 'scoped_to',
    'semantic_delta_from', 'supersedes', 'supports', 'supports_answer', 'total_of', 'triggers',
    'updates_state', 'uses_background', 'verified_by', 'violates_constraint'))

_LEADING = re.compile(r'^\s*([a-z_]+)\s*(?:→|->)')
_ARROW = re.compile(r'--([a-z_]+)-->')


def names_relation(text):
    """The relation type a line is built from, or None."""
    match = _LEADING.match(text) or _ARROW.search(text)
    if match and match.group(1) in RELATION_TYPES:
        return match.group(1)
    return None
