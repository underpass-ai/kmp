"""measure: whole-unit counts per exposure and per-stage totals, one series at a time."""
from ..domain.accounting import aggregate, measure_unit
from ..domain.exposure import ExposureKind, Stage
from ..representations import units

SIDES = (ExposureKind.REQUEST, ExposureKind.RESPONSE, ExposureKind.NOTIFICATION)


def _totals(rows, exposures):
    """Totals by stage and exposure kind, plus the journey total, for one series."""
    by_stage = {}
    for stage in Stage:
        kinds = {}
        for kind in SIDES:
            chosen = [m for m in rows if exposures[m.exposure_id].stage is stage
                      and exposures[m.exposure_id].kind is kind]
            if chosen:
                kinds[kind.value] = aggregate(chosen).as_dict()
        if kinds:
            by_stage[stage.value] = kinds
    return {'by_stage': by_stage, 'all': aggregate(rows).as_dict()}


def measure_journey(trace, counters, representations, max_unit_bytes):
    exposures = {item.exposure.exposure_id: item.exposure for item in trace.observed}
    measurements, totals = [], {}
    for counter in counters:
        series = totals.setdefault(counter.identity.encoding, {})
        for representation in representations:
            rows = [measure_unit(counter, unit, max_unit_bytes)
                    for item in trace.observed
                    for unit in units(representation, item.exposure, item.payload, item.raw_text)]
            series[representation.value] = _totals(rows, exposures)
            measurements += rows
    return {**trace.summary(), 'complete': not trace.failures,
            'exposures': [e.as_dict() for e in exposures.values()],
            'measurements': [m.as_dict() for m in measurements], 'totals': totals}
