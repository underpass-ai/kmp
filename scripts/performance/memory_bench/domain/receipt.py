"""Whether a write landed: a port of kmp-testkit's WriteReceipt (src/write_receipt.rs).

A dry run never counts; a receipt that says `accepted` counts only as
`committed` or `replayed` (`unconfirmed` is not a commit); the canonical
ingest has no review gate and answers `memory.read_after_write_ready` instead.
A store build or a judged case seeds nothing until every receipt passes.
"""
LANDED = ('committed', 'replayed')


def _field(value, key):
    return value.get(key) if isinstance(value, dict) else None


def is_accepted(structured):
    if _field(structured, 'dry_run') is True:
        return False
    accepted = _field(structured, 'accepted')
    if isinstance(accepted, bool):
        return accepted and _field(structured, 'status') in LANDED
    return _field(_field(structured, 'memory'), 'read_after_write_ready') is True


def needs_review(structured):
    return _field(structured, 'status') == 'needs_review'


def review_action(structured):
    """(tool, arguments) of `next_actions[0]`, executed verbatim by a fixture resolving its own review."""
    actions = _field(structured, 'next_actions')
    action = actions[0] if isinstance(actions, list) and actions else None
    tool = _field(action, 'tool')
    return (tool, _field(action, 'arguments')) if isinstance(tool, str) else None
