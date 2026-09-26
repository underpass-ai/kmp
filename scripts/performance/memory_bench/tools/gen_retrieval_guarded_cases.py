"""Appends BT02's anchored and negative cases to retrieval_cases.json.

Idempotent: drops any case carrying a `kind` before appending, so the 35
originals (which carry none) are never touched.
"""
import json
import sys

PATH = sys.argv[1]


def entry(about, key, kind, text, day):
    return {
        "id": f"{about}:{key}",
        "kind": kind,
        "text": text,
        "coordinates": [
            {
                "dimension": "work",
                "scope_id": "work:main",
                "occurred_at": f"2026-08-{day:02d}T09:00:00Z",
                "valid_from": f"2026-08-{day:02d}T09:00:00Z",
                "sequence": day,
            }
        ],
    }


def memory(about, rows):
    return {
        "dimensions": [{"id": "work:main", "kind": "work"}],
        "entries": [entry(about, k, kind, text, i + 1) for i, (k, kind, text) in enumerate(rows)],
    }


# One synthetic project whose shape follows the real failures: short
# identifiers with a dot, issue numbers, and attributes that occur in the
# about but never beside the anchor the question names.
ATLAS = "project:atlas"
ATLAS_ROWS = [
    ("c64-adapter", "decision", "C6.4 local execution adapter runs ceremony steps inside the operator sandbox."),
    ("c64-contract", "decision", "C6.4 contract: the local execution adapter returns a receipt within thirty seconds."),
    ("c64-pending", "observation", "C6.4 pending work: the retry policy of the local execution adapter is not implemented yet."),
    ("c65-scheduler", "decision", "C6.5 remote scheduler dispatches queued ceremonies to the worker pool."),
    ("i188-preflight", "observation", "Issue #188 pause resume preflight checks the journal before a ceremony resumes."),
    ("i188-deploy", "observation", "Issue #188 was deployed to the staging environment on Tuesday."),
    ("kube", "observation", "The Kubernetes cluster for the billing service was upgraded to version 1.30."),
    ("c7-migration", "decision", "C7 migration moves the artifact store to object storage."),
    ("c613-scope", "decision", "C6.13 scope covers the pause and resume verbs of the ceremony runner."),
    ("c613-limit", "decision", "C6.13 limit: a paused ceremony expires after seven days."),
    ("c7-limit", "decision", "C7 limit: the artifact store rejects uploads above two gigabytes."),
    ("menu", "observation", "The canteen menu was posted on the board."),
]


def ref(key, about=ATLAS):
    return f"{about}:{key}"


def case(cid, kind, probes, question, judged, mem, about=ATLAS, forbidden=(), absent=(), **extra):
    out = {
        "id": cid,
        "kind": kind,
        "probes": probes,
        "about": about,
        "question": question,
        "answer_policy": "evidence_or_unknown",
        "judged": list(judged),
    }
    if forbidden:
        out["forbidden"] = list(forbidden)
    if absent:
        out["absent"] = list(absent)
    out["memory"] = mem
    out.update(extra)
    return out


atlas = memory(ATLAS, ATLAS_ROWS)

# Twins of answer_ranker.rs:1662 and :1752, with an anchor added, in both
# polarities: the answer stored (positive) and only the context stored
# (negative). The unanchored originals stay untouched in that file.
CI = "project:ci"
ci_context = ("ci-context", "observation", "CI workflow #42 concluded and the remote branch remained present.")
ci_engine = ("ci-engine", "decision", "CI workflow #42 ran its migrations against the PostgreSQL database engine.")
ci_filler = ("ci-filler", "observation", "CI workflow #51 published the nightly documentation site.")
KERNEL = "project:kernel"
kernel_prefer = ("prefer", "observation", "The kernel prefer mode #7 delivered a decision from another workflow.")
kernel_prefix = ("prefix", "decision", "Kernel prefix #7 was a deliberate decision recorded by the storage team.")
kernel_filler = ("filler", "observation", "The kernel prefer mode #9 delivered a decision from another workflow.")

CI_Q = "Which database engine was used when CI workflow #42 concluded and the remote branch remained present?"
KERNEL_Q = "Was kernel prefix #7 a deliberate decision?"

BILLING = "project:billing"

cases = [
    # Positives.
    case("anchored-enumerative-c64", "enumerative_anchored",
         "Real usage shape: a facet enumeration over an identifier with a dot. Every facet is stored beside the anchor.",
         "Which decisions, contracts, limits and pending work exist for C6.4?",
         [ref("c64-adapter"), ref("c64-contract"), ref("c64-pending")], atlas),
    case("anchored-enumerative-c613", "enumerative_anchored",
         "Enumeration over a two-digit dotted identifier whose neighbour C6.1x shares the prefix.",
         "What scope and limits are recorded for C6.13?",
         [ref("c613-scope"), ref("c613-limit")], atlas),
    case("anchored-singular-i188", "singular_anchored",
         "Singular question over an issue number whose answer is stored beside it.",
         "Where was issue #188 deployed?",
         [ref("i188-deploy")], atlas),
    case("anchored-twin-1662-positive", "anchored_twin_positive",
         "Anchored twin of answer_ranker.rs:1662 with the attribute stored for the subject: the strict policy must answer.",
         CI_Q, [ref("ci-engine", CI)], memory(CI, [ci_context, ci_engine, ci_filler]), about=CI),
    case("anchored-twin-1752-positive", "anchored_twin_positive",
         "Anchored twin of answer_ranker.rs:1752 with the decision stored for the subject: the strict policy must answer.",
         KERNEL_Q, [ref("prefix", KERNEL)], memory(KERNEL, [kernel_prefix, kernel_filler]), about=KERNEL),
    # Guarded positives: an answer exists, and citing the excluded anchor is false.
    case("negated-anchor-c613-without-c7", "negated_anchor",
         "Positive with an exclusion: the C7 limit shares every attribute word and must not be cited.",
         "What limits apply to C6.13, excluding any C7 work?",
         [ref("c613-limit")], atlas, forbidden=[ref("c7-limit"), ref("c7-migration")]),
    case("negated-anchor-c64-without-c65", "negated_anchor",
         "Positive with an exclusion over neighbouring dotted identifiers: nothing about C6.5 may be cited.",
         "Which pending work exists for C6.4, excluding the C6.5 scheduler?",
         [ref("c64-pending")], atlas, forbidden=[ref("c65-scheduler")]),
    # Negatives: the store holds no answer; any ANSWER is false.
    case("anchored-twin-1662-negative", "singular_anchored_twin",
         "Anchored twin of answer_ranker.rs:1662: only the context is stored for CI workflow #42, never a database engine.",
         CI_Q, [], memory(CI, [ci_context, ci_filler]), about=CI, absent=["database", "engine"]),
    case("anchored-twin-1752-negative", "singular_anchored_twin",
         "Anchored twin of answer_ranker.rs:1752: #7 is stored with prefer/delivered, never with prefix/deliberate.",
         KERNEL_Q, [], memory(KERNEL, [kernel_prefer, kernel_filler]), about=KERNEL, absent=["prefix", "deliberate"]),
    case("near-miss-kubernetes-i188", "near_miss_attribute",
         "Kubernetes occurs in the about and #188 occurs in the about, never in one entry.",
         "Which Kubernetes cluster was issue #188 deployed to?",
         [], atlas, absent=["kubernetes"]),
    case("near-miss-object-storage-i188", "near_miss_attribute",
         "Object storage occurs only beside C7; the question asks it of issue #188.",
         "Which object storage does issue #188 use?",
         [], atlas, absent=["object", "storage"]),
    case("anchor-neighbor-c65-receipt", "anchor_neighbor_existing",
         "The receipt deadline is stored for C6.4 only; C6.5 exists and has none.",
         "Within how many seconds does the C6.5 remote scheduler return a receipt?",
         [], atlas, forbidden=[ref("c64-contract")], absent=["receipt"]),
    case("anchor-neighbor-c65-retry", "anchor_neighbor_existing",
         "The retry policy is stored for C6.4 only; C6.5 exists and has none.",
         "What retry policy is pending for C6.5?",
         [], atlas, forbidden=[ref("c64-pending")], absent=["retry"]),
    case("anchor-absent-c624-short", "anchor_absent",
         "Observed in use at v0.23.0: C6.24 does not exist and C6.4 was cited at high confidence.",
         "C6.24 local execution adapter",
         [], atlas, forbidden=[ref("c64-adapter")], absent=["c6.24", "24"]),
    case("anchor-absent-i288-short", "anchor_absent",
         "Observed in use at v0.23.0: issue #288 does not exist and #188 was cited at high confidence.",
         "issue #288 pause resume preflight",
         [], atlas, forbidden=[ref("i188-preflight")], absent=["#288", "288"]),
    case("anchor-absent-c624-enumerative", "anchor_absent",
         "Long enumerative form of the absent C6.24 anchor over a store where C6.4 answers every facet.",
         "Which decisions, contracts, limits and pending work exist for C6.24?",
         [], atlas, absent=["c6.24", "24"]),
    case("anchor-absent-i288-enumerative", "anchor_absent",
         "Long enumerative form of the absent #288 anchor over a store where #188 answers every facet.",
         "What decisions, deployments and preflight checks were recorded for issue #288?",
         [], atlas, absent=["#288", "288"]),
    case("cross-about-anchor-c92", "cross_about_anchor",
         "C9.2 is stored only in another about; the question stays in this one.",
         "What does the C9.2 billing export include?",
         [], atlas, absent=["c9.2", "c9", "2"],
         memories=[{"about": BILLING, "memory": memory(BILLING, [
             ("c92-export", "decision", "C9.2 billing export includes invoices and credit notes."),
         ])}]),
]

with open(PATH, encoding="utf-8") as handle:
    data = json.load(handle)
data["cases"] = [c for c in data["cases"] if "kind" not in c] + cases
with open(PATH, "w", encoding="utf-8") as handle:
    handle.write(json.dumps(data, indent=2, ensure_ascii=False) + "\n")
print(len(data["cases"]), "cases")
