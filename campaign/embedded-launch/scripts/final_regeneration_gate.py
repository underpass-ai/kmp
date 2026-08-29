#!/usr/bin/env python3
"""Clean-room, fail-closed regeneration gate for KMP Embedded launch masters.

The gate never writes to the checkout. It exports committed HEAD into an empty,
explicit repository-local scratch directory, renders there, and compares the
result with the committed distribution and transitive reproduction evidence.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tarfile
from typing import Any, Iterable


SCRIPT = pathlib.Path(__file__).resolve()
CAMPAIGN = SCRIPT.parents[1]
ROOT = CAMPAIGN.parents[1]
REPO_TMP = ROOT / "tmp"
COMPARISON_RELATIVE = pathlib.PurePosixPath(
    "campaign/embedded-launch/evidence-pack/reproduction/clean-render-comparison.json"
)
CONTRACT = "kmp.embedded-launch.clean-render-comparison.v1"


class GateFailure(RuntimeError):
    """A final artifact cannot be proved from the committed source tree."""


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_json(value: object) -> str:
    encoded = json.dumps(
        value, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def read_json(path: pathlib.Path, *, label: str) -> dict[str, Any]:
    if not path.is_file():
        raise GateFailure(f"missing {label}: {path}")
    try:
        body = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise GateFailure(f"unreadable {label}: {path}: {error}") from error
    if not isinstance(body, dict):
        raise GateFailure(f"{label} is not a JSON object: {path}")
    return body


def run(
    command: list[str],
    *,
    cwd: pathlib.Path,
    log: pathlib.Path,
) -> None:
    environment = os.environ.copy()
    environment["PYTHONDONTWRITEBYTECODE"] = "1"
    with log.open("a", encoding="utf-8") as stream:
        stream.write("$ " + " ".join(command) + "\n")
        stream.flush()
        result = subprocess.run(
            command,
            cwd=cwd,
            env=environment,
            text=True,
            stdout=stream,
            stderr=subprocess.STDOUT,
            check=False,
        )
        stream.write(f"[exit {result.returncode}]\n")
    if result.returncode:
        raise GateFailure(
            f"command failed with exit {result.returncode}; see {log}: "
            + " ".join(command)
        )


def validate_scratch(raw: pathlib.Path, *, repository_tmp: pathlib.Path = REPO_TMP) -> pathlib.Path:
    """Accept only one empty, real directory strictly below this repo's tmp/."""
    if not raw.is_absolute():
        raw = pathlib.Path.cwd() / raw
    if not raw.exists() or not raw.is_dir():
        raise GateFailure("--scratch must name an existing directory")

    lexical_tmp = repository_tmp.absolute()
    lexical_candidate = raw.absolute()
    tmp = repository_tmp.resolve()
    candidate = raw.resolve()
    if candidate == tmp or tmp not in candidate.parents:
        raise GateFailure(f"--scratch must be strictly below repository tmp/: {tmp}")

    try:
        relative = lexical_candidate.relative_to(lexical_tmp)
    except ValueError as error:
        raise GateFailure(f"--scratch must be below repository tmp/: {tmp}") from error
    cursor = lexical_tmp
    for part in relative.parts:
        cursor = cursor / part
        if cursor.is_symlink():
            raise GateFailure(f"--scratch may not traverse a symlink: {cursor}")
    if any(candidate.iterdir()):
        raise GateFailure("--scratch must be empty")
    return candidate


def safe_extract_archive(archive: pathlib.Path, target: pathlib.Path) -> None:
    target.mkdir()
    with tarfile.open(archive, "r") as bundle:
        for member in bundle.getmembers():
            name = pathlib.PurePosixPath(member.name)
            if name.is_absolute() or ".." in name.parts:
                raise GateFailure(f"git archive contains an unsafe member: {member.name}")
        try:
            bundle.extractall(target, filter="data")
        except TypeError:  # Python 3.11 compatibility; git archive is the trusted producer.
            bundle.extractall(target)


def export_committed_head(scratch: pathlib.Path, *, root: pathlib.Path = ROOT) -> tuple[pathlib.Path, str]:
    checkout = scratch / "checkout"
    archive = scratch / "committed-head.tar"
    head = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=root, check=True, text=True,
        capture_output=True,
    ).stdout.strip()
    subprocess.run(
        ["git", "archive", "--format=tar", "--output", str(archive), "HEAD"],
        cwd=root,
        check=True,
    )
    safe_extract_archive(archive, checkout)
    archive.unlink()
    return checkout, head


def portable_audio_report(report: dict[str, Any]) -> dict[str, Any]:
    """Remove the two renderer-defined scratch paths, and no other field."""
    portable = copy.deepcopy(report)
    for section_name in ("precontrolled", "mix"):
        section = portable.get(section_name)
        if not isinstance(section, dict) or "path" not in section:
            raise GateFailure(f"audio report has no {section_name}.path")
        section.pop("path")
    assert_no_absolute_paths(portable)
    return portable


def portable_distribution_audio(report: Any) -> dict[str, Any]:
    """Remove the renderer-defined distribution path, and no other field."""
    if not isinstance(report, dict) or "path" not in report:
        raise GateFailure("distribution audio report has no path")
    portable = copy.deepcopy(report)
    portable.pop("path")
    assert_no_absolute_paths(portable)
    return portable


def manifest_projection(manifest: dict[str, Any]) -> dict[str, Any]:
    """Return the portable, product-relevant part of a renderer manifest."""
    reports = manifest.get("audio_premix_reports")
    masters = manifest.get("masters")
    if not isinstance(reports, list) or not isinstance(masters, list):
        raise GateFailure("renderer manifest has no audio reports or masters")

    portable_reports: dict[str, Any] = {}
    for report in reports:
        if not isinstance(report, dict) or not isinstance(report.get("master_id"), str):
            raise GateFailure("renderer manifest contains an unidentified audio report")
        master_id = report["master_id"]
        if master_id in portable_reports:
            raise GateFailure(f"renderer manifest repeats audio report {master_id}")
        portable_reports[master_id] = portable_audio_report(report)

    portable_masters: dict[str, Any] = {}
    for master in masters:
        if not isinstance(master, dict) or not isinstance(master.get("path"), str):
            raise GateFailure("renderer manifest contains an unidentified master")
        master_id = pathlib.PurePosixPath(master["path"]).stem
        if master_id in portable_masters:
            raise GateFailure(f"renderer manifest repeats master {master_id}")
        item = copy.deepcopy(master)
        if "audio" in item:
            item["audio"] = portable_distribution_audio(item["audio"])
        else:
            raise GateFailure(f"renderer manifest master {master_id} has no audio report")
        assert_no_absolute_paths(item)
        portable_masters[master_id] = item

    return {
        "schema_version": manifest.get("schema_version"),
        "campaign_id": manifest.get("campaign_id"),
        "status": manifest.get("status"),
        "publishable": manifest.get("publishable"),
        "approval_manifest": manifest.get("approval_manifest"),
        "audio_contract": manifest.get("audio_contract"),
        "audio_premix_reports": portable_reports,
        "masters": portable_masters,
    }


def assert_same(label: str, expected: object, observed: object) -> None:
    if expected != observed:
        raise GateFailure(f"{label} differs from the committed evidence")


def assert_no_absolute_paths(value: Any, *, at: str = "$") -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            assert_no_absolute_paths(item, at=f"{at}.{key}")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            assert_no_absolute_paths(item, at=f"{at}[{index}]")
    elif isinstance(value, str):
        if value.startswith("/") or (len(value) > 2 and value[1:3] in {":/", ":\\"}):
            raise GateFailure(f"portable comparison contains an absolute path at {at}")


def file_binding(path: pathlib.Path, root: pathlib.Path) -> dict[str, Any]:
    if not path.is_file():
        raise GateFailure(f"missing transitive input: {path}")
    return {
        "path": path.relative_to(root).as_posix(),
        "sha256": sha256(path),
        "bytes": path.stat().st_size,
    }


def source_paths(checkout: pathlib.Path, master_ids: Iterable[str]) -> list[pathlib.Path]:
    campaign = checkout / "campaign" / "embedded-launch"
    paths = [
        campaign / "campaign.json",
        campaign / "edl.json",
        campaign / "audio" / "contract.json",
        campaign / "audio" / "cues.tsv",
        campaign / "audio" / "evidence-knot.csd",
        campaign / "audio" / "mix-levels.json",
        campaign / "scripts" / "audio_contract.py",
        campaign / "scripts" / "capture_contract.py",
        campaign / "scripts" / "render-audio.sh",
        campaign / "scripts" / "render-campaign.py",
        campaign / "scripts" / "final-regeneration-gate.sh",
        campaign / "scripts" / "final_regeneration_gate.py",
    ]
    for master_id in master_ids:
        paths.extend(
            [
                campaign / "captions" / f"{master_id}.vtt",
                campaign / "evidence-pack" / "capture" / "promoted" / f"{master_id}.json",
                campaign / "evidence-pack" / "capture" / "raw" / f"{master_id}.mkv",
                campaign / "evidence-pack" / "capture" / "raw" / f"{master_id}.mkv.sha256",
            ]
        )
    return paths


def protected_records(checkout: pathlib.Path) -> dict[str, dict[str, Any]]:
    pack = checkout / "campaign" / "embedded-launch" / "evidence-pack"
    paths: list[pathlib.Path] = []
    for relative in ("qa", "signoffs"):
        root = pack / relative
        if root.exists():
            paths.extend(path for path in root.rglob("*") if path.is_file())
    for relative in ("critic-input.json", "publication-manifest.json"):
        path = pack / relative
        if path.is_file():
            paths.append(path)
    return {
        path.relative_to(checkout).as_posix(): file_binding(path, checkout)
        for path in sorted(paths)
    }


def verify_audio_evidence(checkout: pathlib.Path, build: pathlib.Path) -> dict[str, Any]:
    campaign = checkout / "campaign" / "embedded-launch"
    committed = campaign / "evidence-pack" / "audio"
    rendered = build / "audio"
    pairs = {
        "provenance.json": (committed / "provenance.json", rendered / "provenance.json"),
        "cues.tsv": (committed / "cues.tsv", rendered / "cues.tsv"),
        "SHA256SUMS": (committed / "SHA256SUMS", rendered / "SHA256SUMS"),
    }
    report: dict[str, Any] = {}
    for name, (expected, observed) in pairs.items():
        if not expected.is_file() or not observed.is_file():
            raise GateFailure(f"missing committed or regenerated audio evidence: {name}")
        expected_bytes = expected.read_bytes()
        observed_bytes = observed.read_bytes()
        if expected_bytes != observed_bytes:
            raise GateFailure(f"regenerated audio evidence differs: {name}")
        report[name] = {
            "path": expected.relative_to(checkout).as_posix(),
            "sha256": sha256(observed),
            "bytes": observed.stat().st_size,
        }
    source_cues = campaign / "audio" / "cues.tsv"
    if source_cues.read_bytes() != (committed / "cues.tsv").read_bytes():
        raise GateFailure("committed audio cue map differs from its source")
    return report


def _canonical_pcm_reports(projected: dict[str, Any]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for master_id, report in projected["audio_premix_reports"].items():
        try:
            result[master_id] = {
                "precontrolled": report["precontrolled"]["canonical_pcm"],
                "mix": report["mix"]["canonical_pcm"],
            }
        except (KeyError, TypeError) as error:
            raise GateFailure(f"{master_id} has no canonical PCM bindings") from error
    return result


def build_comparison(
    checkout: pathlib.Path,
    campaign: dict[str, Any],
    expected_manifest: dict[str, Any],
    regenerated_manifest: dict[str, Any],
    build: pathlib.Path,
    bindings: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    expected_projection = manifest_projection(expected_manifest)
    regenerated_projection = manifest_projection(regenerated_manifest)
    assert_same("portable renderer manifest", expected_projection, regenerated_projection)

    master_ids = [item.get("id") for item in campaign.get("masters", [])]
    if not master_ids or not all(isinstance(item, str) for item in master_ids):
        raise GateFailure("campaign.json has no identified masters")
    expected_ids = set(master_ids)
    if set(regenerated_projection["masters"]) != expected_ids:
        raise GateFailure("regenerated master inventory differs from campaign.json")

    masters: dict[str, Any] = {}
    for master_id in master_ids:
        path = checkout / "docs" / "assets" / "campaign" / "kmp-embedded" / f"{master_id}.mp4"
        manifest_item = regenerated_projection["masters"][master_id]
        if sha256(path) != manifest_item.get("sha256"):
            raise GateFailure(f"regenerated {master_id} hash differs from its manifest")
        raw = checkout / "campaign" / "embedded-launch" / "evidence-pack" / "capture" / "raw" / f"{master_id}.mkv"
        cue_gate = manifest_item.get("cue_anchor_gate", {})
        masters[master_id] = {
            "path": path.relative_to(checkout).as_posix(),
            "sha256": sha256(path),
            "bytes": path.stat().st_size,
            "picture_source_sha256": sha256(raw),
            "anchors_sha256": cue_gate.get("anchors_sha256"),
            "cue_resolution_sha256": cue_gate.get("cue_resolution_sha256"),
            "distribution_audio_pcm": manifest_item.get("audio", {})
            .get("stream", {})
            .get("decoded_pcm"),
        }

    result = {
        "contract": CONTRACT,
        "campaign_id": campaign.get("campaign_id"),
        "source_bindings": bindings,
        "renderer_manifest_projection_sha256": sha256_json(regenerated_projection),
        "audio": {
            "evidence": verify_audio_evidence(checkout, build),
            "premix_canonical_pcm": _canonical_pcm_reports(regenerated_projection),
        },
        "masters": masters,
        "passed": True,
    }
    assert_no_absolute_paths(result)
    return result


def compare_expected_report(expected: dict[str, Any], observed: dict[str, Any]) -> None:
    if expected.get("contract") != CONTRACT:
        raise GateFailure("clean-render comparison has the wrong contract")
    if expected.get("passed") is not True:
        raise GateFailure("clean-render comparison is not a passing record")
    assert_no_absolute_paths(expected)
    assert_same("clean-render comparison", expected, observed)


def static_gate_commands(campaign_root: pathlib.Path) -> tuple[list[str], ...]:
    scripts = campaign_root / "scripts"
    return (
        [sys.executable, str(scripts / "build-evidence-manifest.py"), "check"],
        [sys.executable, str(scripts / "panel_contract.py"), "check"],
        [sys.executable, str(scripts / "verify-final-media.py")],
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Regenerate committed KMP Embedded masters in an isolated checkout."
    )
    parser.add_argument("--scratch", required=True, type=pathlib.Path)
    args = parser.parse_args(argv)

    try:
        scratch = validate_scratch(args.scratch)
        checkout, head = export_committed_head(scratch)
        log = scratch / "regeneration.log"
        build = scratch / "build"
        build.mkdir()
        campaign_root = checkout / "campaign" / "embedded-launch"
        pack = campaign_root / "evidence-pack"
        output = checkout / "docs" / "assets" / "campaign" / "kmp-embedded"
        campaign = read_json(campaign_root / "campaign.json", label="campaign brief")
        expected_manifest_path = output / "manifest.json"
        expected_manifest = read_json(expected_manifest_path, label="committed renderer manifest")
        expected_manifest_bytes = expected_manifest_path.read_bytes()
        expected_report_path = checkout / COMPARISON_RELATIVE
        expected_report = (
            read_json(expected_report_path, label="committed clean-render comparison")
            if expected_report_path.is_file()
            else None
        )
        before_protected = protected_records(checkout)

        # A finished candidate gets a cheap static/security rejection before
        # the expensive render. The missing-report bootstrap remains fail-closed
        # but is allowed to produce only a scratch comparison for human review.
        if expected_report is not None and (pack / "manifest.json").is_file():
            for command in static_gate_commands(campaign_root):
                run(command, cwd=checkout, log=log)

        master_ids = [item.get("id") for item in campaign.get("masters", [])]
        if not master_ids or not all(isinstance(item, str) for item in master_ids):
            raise GateFailure("campaign.json has no identified masters")
        before_sources = {
            item.relative_to(checkout).as_posix(): file_binding(item, checkout)
            for item in source_paths(checkout, master_ids)
        }

        expected_master_hashes = {
            item["id"]: sha256(output / f"{item['id']}.mp4")
            for item in campaign.get("masters", [])
        }
        run(
            [
                sys.executable, str(campaign_root / "scripts" / "render-campaign.py"),
                str(pack / "capture" / "raw"), str(build),
            ],
            cwd=checkout,
            log=log,
        )
        regenerated_manifest = read_json(output / "manifest.json", label="regenerated renderer manifest")
        after_protected = protected_records(checkout)
        assert_same("protected human review records", before_protected, after_protected)
        after_sources = {
            item.relative_to(checkout).as_posix(): file_binding(item, checkout)
            for item in source_paths(checkout, master_ids)
        }
        assert_same("committed render sources", before_sources, after_sources)

        for master_id, expected_hash in expected_master_hashes.items():
            observed_hash = sha256(output / f"{master_id}.mp4")
            if observed_hash != expected_hash:
                raise GateFailure(
                    f"regenerated master differs byte-for-byte: {master_id} "
                    f"({observed_hash} != {expected_hash})"
                )

        report = build_comparison(
            checkout, campaign, expected_manifest, regenerated_manifest, build,
            before_sources,
        )
        report_path = scratch / "final-regeneration-report.json"
        report_path.write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        if expected_report is None:
            raise GateFailure(
                "committed clean-render comparison is missing; the independently "
                f"regenerated candidate is available for review at {report_path}"
            )
        compare_expected_report(expected_report, report)

        # The renderer's manifest contains scratch-local diagnostic paths. Its
        # portable projection was proved above; restore the committed bytes so
        # the transitive evidence-manifest hash can be checked without weakening
        # that repository-facing artifact contract.
        expected_manifest_path.write_bytes(expected_manifest_bytes)

        # Re-run all static/transitive gates inside the exported tree after the
        # regenerated masters replaced the committed copies.
        for command in static_gate_commands(campaign_root):
            run(command, cwd=checkout, log=log)

        print(
            f"final regeneration gate passed: committed HEAD {head}, "
            f"{len(expected_master_hashes)} byte-identical masters; report {report_path}"
        )
        return 0
    except (GateFailure, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"final regeneration gate failed: {error}") from error


if __name__ == "__main__":
    raise SystemExit(main())
