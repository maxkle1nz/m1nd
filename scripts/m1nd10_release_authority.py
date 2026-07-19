#!/usr/bin/env python3
"""Fail-closed remote nonexistence checks for an M1ND release version."""

from __future__ import annotations

import argparse
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable


class ReleaseAuthorityError(RuntimeError):
    pass


PUBLISHED_CRATES = {"m1nd-core", "m1nd-control", "m1nd-ingest", "m1nd-mcp"}
SEMVER_RE = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?")


def http_status(url: str, headers: dict[str, str]) -> int:
    request = urllib.request.Request(url, headers=headers, method="GET")
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            return response.status
    except urllib.error.HTTPError as error:
        return error.code
    except urllib.error.URLError as error:
        raise ReleaseAuthorityError(f"remote authority probe failed closed: {error.reason}") from error


def crate_version_map(values: list[str]) -> dict[str, str]:
    result: dict[str, str] = {}
    for value in values:
        name, separator, version = value.partition("=")
        if (
            separator != "="
            or name not in PUBLISHED_CRATES
            or name in result
            or not SEMVER_RE.fullmatch(version)
        ):
            raise ReleaseAuthorityError(f"invalid or duplicate crate version binding: {value!r}")
        result[name] = version
    if set(result) != PUBLISHED_CRATES:
        raise ReleaseAuthorityError(
            f"crate version set mismatch: expected={sorted(PUBLISHED_CRATES)}, actual={sorted(result)}"
        )
    return result


def targets(
    repository: str,
    tag: str,
    version: str,
    github_token: str,
    crate_versions: dict[str, str],
) -> list[tuple[str, str, dict[str, str]]]:
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ReleaseAuthorityError(f"invalid GitHub repository identity: {repository!r}")
    if tag != f"v{version}":
        raise ReleaseAuthorityError(f"tag {tag!r} does not exactly match version {version!r}")
    if not SEMVER_RE.fullmatch(version):
        raise ReleaseAuthorityError(f"invalid release version: {version!r}")
    if not github_token:
        raise ReleaseAuthorityError("GitHub token is required for the release nonexistence probe")

    owner, name = repository.split("/", 1)
    github_headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {github_token}",
        "User-Agent": "m1nd-release-authority/1",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    public_headers = {"Accept": "application/json", "User-Agent": "m1nd-release-authority/1"}
    release_url = (
        f"https://api.github.com/repos/{urllib.parse.quote(owner, safe='')}/"
        f"{urllib.parse.quote(name, safe='')}/releases/tags/{urllib.parse.quote(tag, safe='')}"
    )
    npm_package = urllib.parse.quote("@maxkle1nz/m1nd", safe="")
    npm_url = f"https://registry.npmjs.org/{npm_package}/{urllib.parse.quote(version, safe='')}"
    result = [("github_release", release_url, github_headers), ("npm_version", npm_url, public_headers)]
    if set(crate_versions) != PUBLISHED_CRATES or any(
        not SEMVER_RE.fullmatch(crate_version) for crate_version in crate_versions.values()
    ):
        raise ReleaseAuthorityError("crate version authority set is incomplete or invalid")
    for crate in ("m1nd-core", "m1nd-control", "m1nd-ingest", "m1nd-mcp"):
        crate_version = crate_versions[crate]
        result.append(
            (
                f"crates_version:{crate}",
                f"https://crates.io/api/v1/crates/{crate}/{urllib.parse.quote(crate_version, safe='')}",
                public_headers,
            )
        )
    return result


def require_nonexistent(
    probes: list[tuple[str, str, dict[str, str]]],
    status_for: Callable[[str, dict[str, str]], int] = http_status,
) -> None:
    for label, url, headers in probes:
        status = status_for(url, headers)
        if status == 404:
            continue
        if status == 200:
            raise ReleaseAuthorityError(f"{label} already exists; immutable release refused")
        raise ReleaseAuthorityError(
            f"{label} nonexistence is NOT_PROVEN (HTTP {status}); release refused"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--github-token", required=True)
    parser.add_argument("--crate-version", action="append", required=True)
    args = parser.parse_args()
    try:
        crate_versions = crate_version_map(args.crate_version)
        require_nonexistent(
            targets(
                args.repository,
                args.tag,
                args.version,
                args.github_token,
                crate_versions,
            )
        )
    except ReleaseAuthorityError as error:
        print(f"release authority refused: {error}", file=sys.stderr)
        return 1
    print("release/version nonexistence proven for GitHub, npm, and crates.io")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
