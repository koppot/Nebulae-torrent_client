#!/usr/bin/env python3
"""Fetch and verify the pinned third-party inputs of the Windows build.

Reads scripts/windows/inputs.lock.json. A file is accepted only when its size and
SHA-256 match the lock; a mismatch deletes the download and fails. No credentials
are used, nothing outside --cache is written.

    fetch_inputs.py --cache DIR [--only NAME ...] [--skip-optional]
                    [--offline | --verify-only] [--list]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

DEFAULT_LOCK = Path(__file__).with_name("inputs.lock.json")
_CHUNK = 1 << 20
_RETRY_DELAYS = (2, 5, 10)


class FetchError(RuntimeError):
    pass


def load_lock(path: Path = DEFAULT_LOCK) -> dict[str, dict]:
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    if data.get("schema") != 1:
        raise FetchError(f"{path}: unsupported lock schema {data.get('schema')!r}")
    inputs = data["inputs"]
    for name, e in inputs.items():
        digest = e.get("sha256", "")
        if len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
            raise FetchError(f"{name}: sha256 must be 64 lowercase hex characters")
        if not isinstance(e.get("bytes"), int) or e["bytes"] <= 0:
            raise FetchError(f"{name}: bytes must be a positive integer")
        if not e["url"].startswith("https://"):
            raise FetchError(f"{name}: only https URLs are allowed")
        if os.sep in e["file"] or "/" in e["file"] or e["file"] in ("", ".", ".."):
            raise FetchError(f"{name}: file must be a plain file name")
    return inputs


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(_CHUNK), b""):
            h.update(chunk)
    return h.hexdigest()


def matches(path: Path, entry: dict) -> bool:
    return path.is_file() and path.stat().st_size == entry["bytes"] and sha256_file(path) == entry["sha256"]


def _open(url: str):
    req = urllib.request.Request(url, headers={"User-Agent": "nebulae-windows-build/1"})
    return urllib.request.urlopen(req, timeout=60)


def _download(entry: dict, part: Path) -> None:
    h = hashlib.sha256()
    total = 0
    limit = entry["bytes"]
    with _open(entry["url"]) as resp, open(part, "wb") as out:
        while True:
            chunk = resp.read(_CHUNK)
            if not chunk:
                break
            total += len(chunk)
            if total > limit:
                raise FetchError(f"{entry['file']}: larger than the pinned {limit} bytes; aborted")
            h.update(chunk)
            out.write(chunk)
    if total != limit:
        raise FetchError(f"{entry['file']}: got {total} bytes, pinned size is {limit}")
    if h.hexdigest() != entry["sha256"]:
        raise FetchError(f"{entry['file']}: SHA-256 {h.hexdigest()} does not match the pin {entry['sha256']}")


def ensure(name: str, entry: dict, cache: Path, offline: bool = False) -> Path:
    """Return the verified cache path of one input, downloading it when needed."""
    cache.mkdir(parents=True, exist_ok=True)
    dest = cache / entry["file"]
    if matches(dest, entry):
        return dest
    if dest.exists():
        dest.unlink()
    if offline:
        raise FetchError(f"{name}: {entry['file']} is not in the cache and --offline was given")
    part = dest.with_name(dest.name + ".part")
    last: Exception | None = None
    for attempt, delay in enumerate((0, *_RETRY_DELAYS)):
        if delay:
            time.sleep(delay)
        try:
            _download(entry, part)
            os.replace(part, dest)
            return dest
        except FetchError:
            part.unlink(missing_ok=True)
            raise
        except (urllib.error.URLError, TimeoutError, ConnectionError, OSError) as exc:
            part.unlink(missing_ok=True)
            if isinstance(exc, urllib.error.HTTPError) and exc.code < 500:
                raise FetchError(f"{name}: HTTP {exc.code} for {entry['url']}") from exc
            last = exc
    raise FetchError(f"{name}: download failed after retries: {last}")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    ap.add_argument("--cache", type=Path, help="download cache directory")
    ap.add_argument("--only", action="append", default=[], metavar="NAME")
    ap.add_argument("--skip-optional", action="store_true", help="skip inputs marked optional in the lock")
    ap.add_argument("--offline", "--verify-only", action="store_true", dest="offline",
                    help="verify what is in the cache, download nothing")
    ap.add_argument("--list", action="store_true")
    args = ap.parse_args(argv)

    inputs = load_lock(args.lock)
    unknown = [n for n in args.only if n not in inputs]
    if unknown:
        ap.error(f"unknown input(s): {', '.join(unknown)}")
    if args.list:
        for name, e in inputs.items():
            print(f"{name:22} {e['version']:28} {e['bytes']:>12}  {e['sha256']}  {e['file']}")
        return 0
    if not args.cache:
        ap.error("--cache is required")

    failed = 0
    for name, entry in inputs.items():
        if args.only and name not in args.only:
            continue
        if args.skip_optional and entry.get("optional") and name not in args.only:
            continue
        try:
            path = ensure(name, entry, args.cache, args.offline)
            print(f"ok       {name:22} {entry['sha256'][:16]}  {path}")
        except FetchError as exc:
            failed += 1
            print(f"FAILED   {name:22} {exc}", file=sys.stderr)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
