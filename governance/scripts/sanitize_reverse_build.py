#!/usr/bin/env python3
"""
Sanitize reverse-build capture artifacts for sharing.

Default behavior is safe: writes sanitized copies next to the original file
using a `.sanitized` suffix. Use `--in-place` to overwrite originals.

Targets:
- JSON capture files (process/connection snapshots, DNS, etc.)
- CSV capture files (modified files lists)

Redactions:
- User profile paths (C:\\Users\\<name> / C:/Users/<name>)
- Common hostnames like Desktop-<name>...
- Private IPv4 addresses (RFC1918)
- in-addr.arpa reverse lookup records
- Sentry ingestion keys in URLs (sentry_key=...)
"""

from __future__ import annotations

import argparse
import ipaddress
import json
import re
from pathlib import Path
from typing import Any


_RE_WIN_USER = re.compile(r"C:\\Users\\[^\\]+")
_RE_POSIX_USER = re.compile(r"C:/Users/[^/]+")
_RE_HOST_DESKTOP = re.compile(r"Desktop-[^\\s\"/]+")
_RE_IPV4 = re.compile(r"\b\d{1,3}(?:\.\d{1,3}){3}\b")
_RE_SENTRY_KEY = re.compile(r"(sentry_key=)[0-9a-fA-F]+")
_RE_IN_ADDR = re.compile(r"[0-9.]+\.in-addr\.arpa\.?")


def _is_private_ipv4(value: str) -> bool:
    try:
        ip = ipaddress.ip_address(value)
    except ValueError:
        return False
    return isinstance(ip, ipaddress.IPv4Address) and (
        ip.is_private or ip.is_loopback or ip.is_link_local
    )


def _sanitize_string(text: str) -> str:
    # Strip UTF-8 BOM if present as a character (common when we ingest tool output).
    text = text.lstrip("\ufeff")

    text = _RE_WIN_USER.sub(r"C:\\Users\\<user>", text)
    text = _RE_POSIX_USER.sub("C:/Users/<user>", text)
    text = _RE_SENTRY_KEY.sub(r"\1<redacted>", text)
    text = _RE_IN_ADDR.sub("<redacted.in-addr.arpa>", text)
    text = _RE_HOST_DESKTOP.sub("<redacted-hostname>", text)

    def repl_ip(match: re.Match[str]) -> str:
        ip = match.group(0)
        return "<private-ip>" if _is_private_ipv4(ip) else ip

    text = _RE_IPV4.sub(repl_ip, text)
    return text


def _sanitize_json(value: Any) -> Any:
    if isinstance(value, str):
        return _sanitize_string(value)
    if isinstance(value, list):
        return [_sanitize_json(v) for v in value]
    if isinstance(value, dict):
        return {k: _sanitize_json(v) for k, v in value.items()}
    return value


def _write_text(path: Path, content: str, in_place: bool) -> Path:
    out = path if in_place else path.with_suffix(path.suffix + ".sanitized")
    out.write_text(content, encoding="utf-8", newline="\n")
    return out


def sanitize_file(path: Path, in_place: bool) -> Path:
    if path.suffix.lower() == ".json":
        raw = path.read_text(encoding="utf-8-sig")
        try:
            data = json.loads(raw)
        except json.JSONDecodeError:
            # Fall back to text sanitization.
            return _write_text(path, _sanitize_string(raw), in_place=in_place)

        sanitized = _sanitize_json(data)
        content = json.dumps(sanitized, indent=2, ensure_ascii=False) + "\n"
        return _write_text(path, content, in_place=in_place)

    if path.suffix.lower() == ".csv":
        raw = path.read_text(encoding="utf-8-sig", errors="replace")
        return _write_text(path, _sanitize_string(raw), in_place=in_place)

    # Unknown type: do nothing.
    return path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root",
        default="reverse_build",
        help="Root folder to sanitize (default: reverse_build).",
    )
    parser.add_argument(
        "--in-place",
        action="store_true",
        help="Overwrite original files instead of writing `.sanitized` copies.",
    )
    args = parser.parse_args()

    root = Path(args.root)
    if not root.exists():
        raise SystemExit(f"Root not found: {root}")

    targets = [
        p
        for p in root.rglob("*")
        if p.is_file() and p.suffix.lower() in {".json", ".csv"}
    ]

    for path in sorted(targets):
        sanitize_file(path, in_place=args.in_place)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
