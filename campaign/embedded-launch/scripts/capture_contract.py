#!/usr/bin/env python3
"""Portable path and credential rules for promoted OBS evidence."""

from __future__ import annotations

import json
import pathlib
import re


TEXT_SUFFIXES = {
    ".ini",
    ".json",
    ".jsonl",
    ".log",
    ".txt",
    ".typescript",
    ".timing",
}
PRIVATE_KEY = re.compile(r"-----BEGIN (?:[A-Z0-9 ]+ )?PRIVATE KEY-----")
CLEAR_VIEWER_CAPABILITY = re.compile(r"[?&]k=[0-9a-f]{64}(?:[^0-9a-f]|$)", re.IGNORECASE)
CLEAR_OBS_PASSWORD = re.compile(r"^ServerPassword=(?!<ephemeral-redacted>$).+$", re.MULTILINE)
KNOWN_TOKEN = re.compile(
    r"(?:github_pat_[A-Za-z0-9_]{20,}|gh[pousr]_[A-Za-z0-9]{30,}|"
    r"sk-(?:proj-)?[A-Za-z0-9_-]{20,}|AKIA[0-9A-Z]{16})"
)
FORBIDDEN_JSON_KEYS = {
    "api_key",
    "apikey",
    "authorization",
    "cookie",
    "password",
    "refresh_token",
    "secret",
    "set-cookie",
    "token",
}


def repo_relative(path: pathlib.Path, root: pathlib.Path) -> str:
    """Return a stable POSIX path inside *root*, rejecting external targets."""
    root = root.resolve()
    resolved = path.resolve()
    try:
        relative = resolved.relative_to(root)
    except ValueError as error:
        raise ValueError(f"path is outside repository root: {resolved}") from error
    return relative.as_posix()


def resolve_repo_path(value: object, root: pathlib.Path) -> pathlib.Path:
    """Resolve a repository-relative POSIX binding without consulting the CWD."""
    if not isinstance(value, str) or not value or "\\" in value:
        raise ValueError("path binding must be a non-empty repository-relative POSIX path")
    relative = pathlib.PurePosixPath(value)
    if relative.is_absolute():
        raise ValueError(f"absolute path binding is forbidden: {value}")
    root = root.resolve()
    resolved = (root / pathlib.Path(*relative.parts)).resolve()
    try:
        resolved.relative_to(root)
    except ValueError as error:
        raise ValueError(f"path binding escapes repository root: {value}") from error
    return resolved


def _json_credential_findings(
    value: object,
    source: str,
    location: str = "$",
    findings: list[str] | None = None,
) -> list[str]:
    if findings is None:
        findings = []
    if isinstance(value, dict):
        for key, item in value.items():
            key_name = str(key).lower()
            item_location = f"{location}.{key}"
            if key_name == "authentication" and item != "redacted":
                findings.append(f"{source}: unredacted OBS authentication at {item_location}")
            elif key_name in FORBIDDEN_JSON_KEYS or key_name.endswith(
                ("_api_key", "_authorization", "_cookie", "_password", "_secret", "_token")
            ):
                if not (isinstance(item, str) and item.startswith("<redacted")):
                    findings.append(f"{source}: secret-shaped JSON field at {item_location}")
            if key_name in {"capability_retained", "cleartext_retained"} and item is not False:
                findings.append(f"{source}: retained ephemeral credential at {item_location}")
            _json_credential_findings(item, source, item_location, findings)
    elif isinstance(value, list):
        for index, item in enumerate(value):
            _json_credential_findings(item, source, f"{location}[{index}]", findings)
    return findings


def credential_findings(run: pathlib.Path) -> list[str]:
    """Return filenames and field locations only; never echo credential values."""
    run = run.resolve()
    findings: list[str] = []
    for file in sorted(run.rglob("*")):
        if not file.is_file():
            continue
        relative = file.relative_to(run).as_posix()
        if file.name.endswith(".private") or ".private/" in relative:
            findings.append(f"{relative}: private capture artifact retained")
            continue
        if file.name.lower() in {"cookies", "cookies-journal", "login data", "web data"}:
            findings.append(f"{relative}: browser credential database retained")
            continue
        if file.suffix.lower() not in TEXT_SUFFIXES:
            continue
        try:
            text = file.read_text(encoding="utf-8", errors="replace")
        except OSError as error:
            findings.append(f"{relative}: cannot inspect text evidence: {error}")
            continue
        if PRIVATE_KEY.search(text):
            findings.append(f"{relative}: private key material retained")
        if CLEAR_VIEWER_CAPABILITY.search(text):
            findings.append(f"{relative}: clear ChronoLoom capability retained")
        if CLEAR_OBS_PASSWORD.search(text):
            findings.append(f"{relative}: clear OBS password retained")
        if KNOWN_TOKEN.search(text):
            findings.append(f"{relative}: known token format retained")
        if file.suffix.lower() in {".json", ".jsonl"}:
            try:
                bodies = (
                    [json.loads(line) for line in text.splitlines() if line.strip()]
                    if file.suffix.lower() == ".jsonl"
                    else [json.loads(text)]
                )
            except json.JSONDecodeError:
                continue
            for index, body in enumerate(bodies, 1):
                location = f"$line{index}" if file.suffix.lower() == ".jsonl" else "$"
                findings.extend(_json_credential_findings(body, relative, location))

    obs_auth = run / "obs-auth.json"
    if not obs_auth.is_file():
        findings.append("obs-auth.json: missing redaction evidence")
    else:
        try:
            auth = json.loads(obs_auth.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            findings.append("obs-auth.json: unreadable redaction evidence")
        else:
            if auth.get("auth_required") is not True:
                findings.append("obs-auth.json: OBS authentication was not required")
            if auth.get("cleartext_retained") is not False:
                findings.append("obs-auth.json: OBS password retention was not disproved")
            if "password_sha256" in auth:
                findings.append("obs-auth.json: password fingerprint retained unnecessarily")
    return sorted(set(findings))
