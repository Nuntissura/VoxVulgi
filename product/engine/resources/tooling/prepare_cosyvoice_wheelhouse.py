#!/usr/bin/env python3
"""Resolve, acquire, validate, and hash-lock the CosyVoice Windows wheelhouse.

The helper runs before the target venv exists.  It never installs, imports, or
executes acquired package code.  Only wheels may enter the wheelhouse; governed
source-only packages are hash-pinned and converted without running build code.
"""

from __future__ import annotations

import argparse
import base64
import csv
import hashlib
import io
import json
import os
import pathlib
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import unicodedata
import urllib.parse
import urllib.request
import zipfile


SCHEMA = "voxvulgi.cosyvoice_wheelhouse.v1"
PYPI_INDEX = "https://pypi.org/simple"
PYPI_HOST = "files.pythonhosted.org"
PYTORCH_INDEX = "https://download.pytorch.org/whl/cpu"
PYTORCH_HOST = "download.pytorch.org"
TORCH_WHEELS = {
    "torch": (
        "2.10.0+cpu",
        "17a09465bab2aab8f0f273410297133d8d8fb6dd84dccbd252ca4a4f3a111847",
    ),
    "torchaudio": (
        "2.10.0+cpu",
        "2a78b81a7a21d39a309e5df6e7473c814f5e2de68ef054788a633e618fe5baa8",
    ),
}
GOVERNED_STATIC = {
    "openai-whisper": (
        "20231117",
        "openai_whisper-20231117-py3-none-any.whl",
        1859178,
        "70ccb12d1f7c0649fd55178224cee4278f065b1579f02082b014c95b258582ee",
    ),
    "wget": (
        "3.2",
        "wget-3.2-py3-none-any.whl",
        23313,
        "dd499a2798da224ba5c8d355274b85e04fdca383b871651b5b312421de7efd79",
    ),
}
GOVERNED_SOURCE_WHEELS = {
    "antlr4-python3-runtime": (
        "4.9.3",
        "antlr4-python3-runtime-4.9.3.tar.gz",
        117034,
        "f224469b4168294902bb1efa80a8bf7855f24c99aef99cbefc1bcd3cce77881b",
        "https://files.pythonhosted.org/packages/3e/38/7859ff46355f76f8d19459005ca000b6e7012f2f1ca597746cbcd1fbfe5e/antlr4-python3-runtime-4.9.3.tar.gz",
    ),
}
SETUPTOOLS_WHEEL_SHA256 = "95b30ddfb717250edb492926c92b5221f7ef3fbcc2b07579bcd4a27da21d0173"
SETUPTOOLS_PTH_SHA256 = "2638ce9e2500e572a5e0de7faed6661eb569d1b696fcba07b0dd223da5f5d224"
COLOREDLOGS_WHEEL_SHA256 = "612ee75c546f53e92e70049c9dbfcc18c935a2b9a53b66085ce9ef6a6e5c0934"
COLOREDLOGS_PTH_SHA256 = "dda83a855986efa5cd87f0248b0199c0086eb0e8e7fece7d6741959c5ce39536"
APPROVED_PTH_HOOKS = {
    SETUPTOOLS_WHEEL_SHA256: {"distutils-precedence.pth": SETUPTOOLS_PTH_SHA256},
    COLOREDLOGS_WHEEL_SHA256: {"coloredlogs.pth": COLOREDLOGS_PTH_SHA256},
}
RESERVED_WINDOWS_NAMES = {
    "con",
    "conin$",
    "conout$",
    "clock$",
    "prn",
    "aux",
    "nul",
    *(f"com{number}" for number in range(1, 10)),
    *(f"lpt{number}" for number in range(1, 10)),
}
PIN_RE = re.compile(r"^([A-Za-z0-9][A-Za-z0-9._-]*)==([A-Za-z0-9][A-Za-z0-9.+_-]*)$")


def canonical_name(value: str) -> str:
    return re.sub(r"[-_.]+", "-", value).lower()


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_path(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def safe_path(raw: str) -> str:
    if not raw or "\x00" in raw or "\\" in raw:
        raise ValueError(f"unsafe wheel path: {raw!r}")
    value = unicodedata.normalize("NFC", raw)
    if value != raw or value.startswith(("/", "//")) or re.match(r"^[A-Za-z]:", value):
        raise ValueError(f"non-canonical wheel path: {raw!r}")
    parts = value.split("/")
    if any(not part or part in {".", ".."} for part in parts):
        raise ValueError(f"unsafe wheel path component: {raw!r}")
    for part in parts:
        if ":" in part or part.endswith((".", " ")):
            raise ValueError(f"Windows-unsafe wheel path component: {raw!r}")
        if part.split(".", 1)[0].casefold() in RESERVED_WINDOWS_NAMES:
            raise ValueError(f"reserved Windows wheel path component: {raw!r}")
    return value


def parse_pins(path: pathlib.Path) -> list[tuple[str, str]]:
    pins: list[tuple[str, str]] = []
    names: set[str] = set()
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        match = PIN_RE.fullmatch(line)
        if match is None:
            raise ValueError(f"non-exact requirement in {path}: {line!r}")
        name, version = match.groups()
        canonical = canonical_name(name)
        if canonical in names:
            raise ValueError(f"duplicate requirement in {path}: {canonical}")
        names.add(canonical)
        pins.append((canonical, version))
    if not pins:
        raise ValueError(f"empty requirement file: {path}")
    return pins


def clean_environment() -> dict[str, str]:
    blocked_prefixes = ("PIP_", "UV_", "HF_", "HUGGINGFACE_", "MODELSCOPE_", "AWS_")
    blocked_exact = {
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
        "requests_ca_bundle",
        "curl_ca_bundle",
        "ssl_cert_file",
        "ssl_cert_dir",
        "git_config_count",
        "git_askpass",
        "ssh_auth_sock",
    }
    environment = {}
    for name, value in os.environ.items():
        upper = name.upper()
        if upper.startswith(blocked_prefixes) or name.casefold() in blocked_exact:
            continue
        environment[name] = value
    environment.update(
        {
            "PIP_CONFIG_FILE": "NUL",
            "PIP_DISABLE_PIP_VERSION_CHECK": "1",
            "PIP_NO_INPUT": "1",
            "PYTHONNOUSERSITE": "1",
        }
    )
    return environment


def run_pip(arguments: list[str], label: str) -> None:
    command = [sys.executable, "-I", "-m", "pip", "--isolated", "--disable-pip-version-check", "--no-input", *arguments]
    result = subprocess.run(command, env=clean_environment(), check=False,
                            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    if result.returncode != 0:
        raise RuntimeError(f"{label} failed with exit code {result.returncode}")


def copy_governed_static(static_dir: pathlib.Path, wheelhouse: pathlib.Path) -> dict[str, pathlib.Path]:
    copied = {}
    for name, (version, filename, size, expected_hash) in GOVERNED_STATIC.items():
        source = static_dir / filename
        if not source.is_file() or source.is_symlink():
            raise ValueError(f"governed static wheel is missing or linked: {source}")
        if source.stat().st_size != size or sha256_path(source) != expected_hash:
            raise ValueError(f"governed static wheel identity mismatch: {source}")
        destination = wheelhouse / filename
        shutil.copyfile(source, destination, follow_symlinks=False)
        if sha256_path(destination) != expected_hash:
            raise ValueError(f"governed static wheel copy mismatch: {destination}")
        copied[name] = destination
    return copied


def write_wheel_member(archive: zipfile.ZipFile, name: str, data: bytes) -> None:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o100644 << 16
    archive.writestr(info, data)


def wheel_record_row(name: str, data: bytes) -> tuple[str, str, str]:
    digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode("ascii")
    return name, f"sha256={digest}", str(len(data))


def acquire_governed_source_archive(destination: pathlib.Path, expected_url: str, expected_size: int, expected_hash: str) -> None:
    parsed = urllib.parse.urlparse(expected_url)
    if (
        parsed.scheme != "https"
        or parsed.hostname != PYPI_HOST
        or parsed.username
        or parsed.password
        or parsed.query
        or parsed.fragment
    ):
        raise ValueError(f"governed source URL escaped the exact PyPI file host: {expected_url}")
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    request = urllib.request.Request(expected_url, headers={"User-Agent": "VoxVulgi-offline-builder/1"})
    with opener.open(request, timeout=120) as response:
        final = urllib.parse.urlparse(response.geturl())
        if final.scheme != "https" or final.hostname != PYPI_HOST or final.username or final.password:
            raise ValueError(f"governed source download redirected outside PyPI: {response.geturl()}")
        digest = hashlib.sha256()
        observed_size = 0
        with destination.open("xb") as stream:
            while True:
                block = response.read(1024 * 1024)
                if not block:
                    break
                observed_size += len(block)
                if observed_size > expected_size:
                    raise ValueError("governed source archive exceeded its exact byte count")
                digest.update(block)
                stream.write(block)
    if observed_size != expected_size or digest.hexdigest() != expected_hash:
        raise ValueError("governed source archive identity mismatch")


def build_antlr_runtime_wheel(source_archive: pathlib.Path, wheelhouse: pathlib.Path) -> dict[str, object]:
    name = "antlr4-python3-runtime"
    version, filename, expected_size, expected_hash, expected_url = GOVERNED_SOURCE_WHEELS[name]
    if source_archive.name != filename or source_archive.stat().st_size != expected_size or sha256_path(source_archive) != expected_hash:
        raise ValueError("governed ANTLR source archive identity mismatch")
    source_prefix = f"antlr4-python3-runtime-{version}/src/antlr4/"
    package_files: dict[str, bytes] = {}
    seen: set[str] = set()
    total = 0
    with tarfile.open(source_archive, mode="r:gz") as source:
        members = source.getmembers()
        if not members or len(members) > 1_000:
            raise ValueError("governed ANTLR source member count outside bound")
        for member in members:
            raw = member.name.replace("\\", "/")
            if raw.startswith(("/", "//")) or re.match(r"^[A-Za-z]:", raw):
                raise ValueError(f"unsafe governed ANTLR source path: {raw!r}")
            parts = raw.split("/")
            if any(not part or part in {".", ".."} for part in parts):
                raise ValueError(f"unsafe governed ANTLR source path component: {raw!r}")
            folded = raw.casefold()
            if folded in seen:
                raise ValueError(f"duplicate governed ANTLR source path: {raw!r}")
            seen.add(folded)
            if member.isdir():
                continue
            if not member.isfile() or member.size < 0 or member.size > 10_000_000:
                raise ValueError(f"non-regular or oversized governed ANTLR source member: {raw!r}")
            total += member.size
            if total > 50_000_000:
                raise ValueError("governed ANTLR source expanded size outside bound")
            if raw.startswith(source_prefix):
                relative = raw.removeprefix(source_prefix)
                if not relative.endswith(".py"):
                    raise ValueError(f"unexpected non-Python ANTLR runtime member: {raw!r}")
                extracted = source.extractfile(member)
                if extracted is None:
                    raise ValueError(f"unable to read governed ANTLR source member: {raw!r}")
                package_files[f"antlr4/{relative}"] = extracted.read()
    required = {
        "antlr4/__init__.py",
        "antlr4/atn/__init__.py",
        "antlr4/dfa/__init__.py",
        "antlr4/error/__init__.py",
        "antlr4/tree/__init__.py",
        "antlr4/xpath/__init__.py",
    }
    if not required.issubset(package_files) or len(package_files) < 50:
        raise ValueError("governed ANTLR runtime package topology is incomplete")

    dist_info = f"antlr4_python3_runtime-{version}.dist-info"
    metadata = (
        "Metadata-Version: 2.1\n"
        "Name: antlr4-python3-runtime\n"
        f"Version: {version}\n"
        "Summary: ANTLR 4.9.3 runtime for Python 3.7\n"
        "Home-page: http://www.antlr.org\n"
        "Author: Eric Vergnaud, Terence Parr, Sam Harwell\n"
        "Author-email: eric.vergnaud@wanadoo.fr\n"
        "License: BSD\n"
        'Requires-Dist: typing ; python_version < "3.5"\n'
        "\n"
    ).encode("utf-8")
    wheel_metadata = (
        "Wheel-Version: 1.0\n"
        "Generator: voxvulgi-hash-pinned-source-wheel-v1\n"
        "Root-Is-Purelib: true\n"
        "Tag: py3-none-any\n"
        "\n"
    ).encode("utf-8")
    entries = dict(sorted(package_files.items()))
    entries[f"{dist_info}/METADATA"] = metadata
    entries[f"{dist_info}/WHEEL"] = wheel_metadata
    record_name = f"{dist_info}/RECORD"
    rows = [wheel_record_row(entry, data) for entry, data in entries.items()]
    record_buffer = io.StringIO(newline="")
    writer = csv.writer(record_buffer, lineterminator="\n")
    writer.writerows([*rows, (record_name, "", "")])
    entries[record_name] = record_buffer.getvalue().encode("utf-8")

    wheel_path = wheelhouse / f"antlr4_python3_runtime-{version}-py3-none-any.whl"
    with zipfile.ZipFile(wheel_path, mode="x") as wheel:
        for entry, data in entries.items():
            write_wheel_member(wheel, entry, data)
    validated = validate_wheel(wheel_path)
    return {
        "name": name,
        "version": version,
        "source_filename": filename,
        "source_bytes": expected_size,
        "source_sha256": expected_hash,
        "source_url": expected_url,
        "wheel_filename": wheel_path.name,
        "wheel_sha256": validated["sha256"],
    }


def build_governed_source_wheels(wheelhouse: pathlib.Path, temporary: pathlib.Path) -> list[dict[str, object]]:
    provenance = []
    for name, (version, filename, expected_size, expected_hash, expected_url) in GOVERNED_SOURCE_WHEELS.items():
        if name != "antlr4-python3-runtime" or version != "4.9.3":
            raise ValueError(f"unsupported governed source-wheel recipe: {name}=={version}")
        source_archive = temporary / filename
        acquire_governed_source_archive(source_archive, expected_url, expected_size, expected_hash)
        provenance.append(build_antlr_runtime_wheel(source_archive, wheelhouse))
    return provenance


def acquire_torch(wheelhouse: pathlib.Path) -> None:
    requirements = [f"{name}=={version}" for name, (version, _) in TORCH_WHEELS.items()]
    run_pip(
        [
            "download",
            "--index-url",
            PYTORCH_INDEX,
            "--no-cache-dir",
            "--no-deps",
            "--only-binary=:all:",
            "--dest",
            str(wheelhouse),
            *requirements,
        ],
        "governed CPU torch wheel acquisition",
    )
    observed = {sha256_path(path): path for path in wheelhouse.glob("*.whl")}
    for name, (version, expected_hash) in TORCH_WHEELS.items():
        path = observed.get(expected_hash)
        if path is None or canonical_name(path.name).find(name) < 0 or version.split("+")[0] not in path.name:
            raise ValueError(f"exact governed {name} wheel was not acquired")


def write_combined_requirements(
    path: pathlib.Path,
    torch_pins: list[tuple[str, str]],
    pypi_pins: list[tuple[str, str]],
) -> list[tuple[str, str]]:
    expected_torch = [(name, version.split("+")[0]) for name, (version, _) in TORCH_WHEELS.items()]
    if torch_pins != expected_torch:
        raise ValueError(f"CPU torch requirement drift: {torch_pins!r}")
    combined = [(name, TORCH_WHEELS[name][0]) for name, _ in torch_pins] + pypi_pins
    if len({name for name, _ in combined}) != len(combined):
        raise ValueError("combined CosyVoice requirements contain duplicate names")
    path.write_text("".join(f"{name}=={version}\n" for name, version in combined), encoding="utf-8", newline="\n")
    return combined


def resolve_report(
    combined_path: pathlib.Path,
    wheelhouse: pathlib.Path,
    report_path: pathlib.Path,
) -> None:
    run_pip(
        [
            "install",
            "--dry-run",
            "--ignore-installed",
            "--index-url",
            PYPI_INDEX,
            "--no-cache-dir",
            "--only-binary=:all:",
            "--find-links",
            str(wheelhouse),
            "--report",
            str(report_path),
            "-r",
            str(combined_path),
        ],
        "CosyVoice all-wheel dependency resolution",
    )


def verify_report(
    report_path: pathlib.Path,
    combined: list[tuple[str, str]],
    wheelhouse: pathlib.Path,
    governed_source_wheels: list[dict[str, object]],
) -> list[dict[str, object]]:
    report = json.loads(report_path.read_text(encoding="utf-8"))
    if report.get("version") != "1" or not isinstance(report.get("install"), list):
        raise ValueError("pip resolution report schema mismatch")
    requested_expected = {name: version for name, version in combined}
    requested_observed: dict[str, str] = {}
    packages = []
    observed_names: set[str] = set()
    root = wheelhouse.resolve()
    for item in report["install"]:
        if item.get("is_direct") is not False:
            raise ValueError("pip report contains a direct/VCS/local requirement")
        if any(key in item.get("download_info", {}) for key in ("vcs_info", "dir_info", "subdirectory")):
            raise ValueError("pip report contains a VCS/directory/subdirectory source")
        metadata = item.get("metadata") or {}
        name = canonical_name(str(metadata.get("name", "")))
        version = str(metadata.get("version", ""))
        if not name or not version or name in observed_names:
            raise ValueError("pip report package identity is missing or duplicated")
        observed_names.add(name)
        download = item.get("download_info") or {}
        raw_url = str(download.get("url", ""))
        parsed = urllib.parse.urlparse(raw_url)
        hashes = ((download.get("archive_info") or {}).get("hashes") or {})
        digest = str(hashes.get("sha256", "")).lower()
        if re.fullmatch(r"[0-9a-f]{64}", digest) is None:
            raise ValueError(f"pip report package has no SHA256: {name}")
        if parsed.scheme == "file":
            local_path = pathlib.Path(urllib.parse.unquote(parsed.path.lstrip("/") if os.name == "nt" else parsed.path)).resolve()
            if os.name == "nt" and re.match(r"^[A-Za-z]:", urllib.parse.unquote(parsed.path).lstrip("/")):
                local_path = pathlib.Path(urllib.parse.unquote(parsed.path).lstrip("/")).resolve()
            if os.path.commonpath((root, local_path)) != str(root) or not local_path.is_file():
                raise ValueError(f"pip report local wheel escaped quarantine: {raw_url}")
            source = "governed_local"
        elif (
            parsed.scheme == "https"
            and parsed.hostname == PYPI_HOST
            and not parsed.username
            and not parsed.password
            and not parsed.fragment
        ):
            source = "pypi"
        else:
            raise ValueError(f"pip report escaped governed wheel sources: {raw_url}")
        if item.get("requested") is True:
            requested_observed[name] = version
        packages.append(
            {
                "name": name,
                "version": version,
                "sha256": digest,
                "source": source,
                "url": raw_url,
            }
        )
    if requested_observed != requested_expected:
        raise ValueError(f"pip report direct requirement mismatch: {requested_observed!r}")
    for name, (version, expected_hash) in TORCH_WHEELS.items():
        row = next((package for package in packages if package["name"] == name), None)
        if row is None or row["version"] != version or row["sha256"] != expected_hash or row["source"] != "governed_local":
            raise ValueError(f"pip report rejected governed {name} identity")
    for name, (version, _, _, expected_hash) in GOVERNED_STATIC.items():
        row = next((package for package in packages if package["name"] == name), None)
        if row is None or row["version"] != version or row["sha256"] != expected_hash or row["source"] != "governed_local":
            raise ValueError(f"pip report rejected governed static {name} identity")
    for governed in governed_source_wheels:
        row = next((package for package in packages if package["name"] == governed["name"]), None)
        if (
            row is None
            or row["version"] != governed["version"]
            or row["sha256"] != governed["wheel_sha256"]
            or row["source"] != "governed_local"
        ):
            raise ValueError(f"pip report rejected governed source wheel {governed['name']} identity")
    return sorted(packages, key=lambda package: str(package["name"]))


def write_hash_lock(path: pathlib.Path, packages: list[dict[str, object]]) -> None:
    text = "".join(
        f"{package['name']}=={package['version']} --hash=sha256:{package['sha256']}\n"
        for package in packages
    )
    path.write_text(text, encoding="utf-8", newline="\n")


def acquire_locked_wheels(lock_path: pathlib.Path, wheelhouse: pathlib.Path) -> None:
    run_pip(
        [
            "download",
            "--index-url",
            PYPI_INDEX,
            "--no-cache-dir",
            "--no-deps",
            "--only-binary=:all:",
            "--require-hashes",
            "--find-links",
            str(wheelhouse),
            "--dest",
            str(wheelhouse),
            "-r",
            str(lock_path),
        ],
        "CosyVoice hash-locked wheel acquisition",
    )


def validate_wheel(path: pathlib.Path) -> dict[str, object]:
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"wheel is missing or linked: {path}")
    stat_result = path.stat()
    if getattr(stat_result, "st_file_attributes", 0) & 0x400:
        raise ValueError(f"wheel is a Windows reparse point: {path}")
    outer_hash = sha256_path(path)
    with zipfile.ZipFile(path, "r") as archive:
        infos = archive.infolist()
        if not infos or len(infos) > 20_000:
            raise ValueError(f"wheel member count outside bound: {path.name}")
        names: list[str] = []
        normalized_paths: list[str] = []
        total = 0
        for item in infos:
            if item.is_dir():
                directory_name = safe_path(item.filename[:-1])
                directory_mode = (item.external_attr >> 16) & 0o170000
                if item.flag_bits & 0x1 or item.file_size != 0 or directory_mode not in {0, stat.S_IFDIR}:
                    raise ValueError(f"wheel directory entry is encrypted or malformed: {item.filename}")
                normalized_paths.append(directory_name)
                continue
            name = safe_path(item.filename)
            names.append(name)
            normalized_paths.append(name)
            if item.flag_bits & 0x1 or item.file_size < 0 or item.file_size > 4_500_000_000:
                raise ValueError(f"wheel member is encrypted or outside size bound: {item.filename}")
            mode = (item.external_attr >> 16) & 0o170000
            if mode not in {0, stat.S_IFREG}:
                raise ValueError(f"non-regular wheel member: {item.filename}")
            if item.compress_size and item.file_size / item.compress_size > 10_000:
                raise ValueError(f"wheel compression ratio outside bound: {item.filename}")
            total += item.file_size
        if total > 12_000_000_000:
            raise ValueError(f"wheel expanded size outside bound: {path.name}")
        if len(normalized_paths) != len({name.casefold() for name in normalized_paths}):
            raise ValueError(f"wheel contains duplicate Windows-normalized paths: {path.name}")
    return {
        "filename": path.name,
        "bytes": stat_result.st_size,
        "sha256": outer_hash,
        "expanded_bytes": total,
        "entry_count": len(names),
    }


def validate_wheelhouse(wheelhouse: pathlib.Path, packages: list[dict[str, object]]) -> list[dict[str, object]]:
    expected_by_hash = {str(package["sha256"]): package for package in packages}
    wheels = sorted(wheelhouse.iterdir(), key=lambda path: path.name.casefold())
    if any(path.suffix.lower() != ".whl" for path in wheels) or len(wheels) != len(packages):
        raise ValueError("wheelhouse contains missing, extra, or non-wheel artifacts")
    inventory = []
    observed_hashes: set[str] = set()
    observed_names: set[str] = set()
    for path in wheels:
        row = validate_wheel(path)
        digest = str(row["sha256"])
        expected = expected_by_hash.get(digest)
        if expected is None:
            raise ValueError(f"wheelhouse artifact escaped resolved lock: {path.name}")
        row["name"] = expected["name"]
        row["version"] = expected["version"]
        if digest in observed_hashes or row["name"] in observed_names:
            raise ValueError(f"wheelhouse package/hash duplicated: {path.name}")
        observed_hashes.add(digest)
        observed_names.add(str(row["name"]))
        inventory.append(row)
    if observed_hashes != set(expected_by_hash):
        raise ValueError("wheelhouse did not materialize the complete resolved hash set")
    return sorted(inventory, key=lambda row: str(row["name"]))


def atomic_write(path: pathlib.Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp", dir=path.parent)
    temporary = pathlib.Path(temporary_name)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    except BaseException:
        temporary.unlink(missing_ok=True)
        raise


def prepare(args: argparse.Namespace) -> None:
    wheelhouse = args.wheelhouse.resolve()
    if wheelhouse.exists():
        raise ValueError(f"wheelhouse destination already exists: {wheelhouse}")
    wheelhouse.mkdir(parents=True)
    try:
        copy_governed_static(args.static_wheels.resolve(), wheelhouse)
        acquire_torch(wheelhouse)
        with tempfile.TemporaryDirectory(prefix="voxvulgi_cosyvoice_resolve_") as temporary_name:
            temporary = pathlib.Path(temporary_name)
            governed_source_wheels = build_governed_source_wheels(wheelhouse, temporary)
            combined_path = temporary / "combined_requirements.txt"
            report_path = temporary / "resolution_report.json"
            combined = write_combined_requirements(
                combined_path,
                parse_pins(args.torch_requirements.resolve()),
                parse_pins(args.pypi_requirements.resolve()),
            )
            resolve_report(combined_path, wheelhouse, report_path)
            packages = verify_report(report_path, combined, wheelhouse, governed_source_wheels)
            lock_temporary = temporary / "requirements.lock.txt"
            write_hash_lock(lock_temporary, packages)
            acquire_locked_wheels(lock_temporary, wheelhouse)
            inventory = validate_wheelhouse(wheelhouse, packages)
            lock_bytes = lock_temporary.read_bytes()
        atomic_write(args.lock.resolve(), lock_bytes)
        manifest = {
            "schema": SCHEMA,
            "platform_contract": "windows_x64_cp311",
            "install_contract": "no_index_only_binary_require_hashes_v1",
            "helper": {
                "filename": pathlib.Path(__file__).name,
                "bytes": pathlib.Path(__file__).stat().st_size,
                "sha256": sha256_path(pathlib.Path(__file__)),
            },
            "indexes": [PYPI_INDEX, PYTORCH_INDEX],
            "governed_source_wheels": governed_source_wheels,
            "lock": {
                "path": str(args.lock.resolve()),
                "bytes": len(lock_bytes),
                "sha256": sha256_bytes(lock_bytes),
            },
            "packages": packages,
            "wheel_inventory": inventory,
        }
        atomic_write(args.manifest.resolve(), (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode("utf-8"))
    except BaseException:
        shutil.rmtree(wheelhouse, ignore_errors=True)
        args.lock.resolve().unlink(missing_ok=True)
        args.manifest.resolve().unlink(missing_ok=True)
        raise


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--torch-requirements", required=True, type=pathlib.Path)
    parser.add_argument("--pypi-requirements", required=True, type=pathlib.Path)
    parser.add_argument("--static-wheels", required=True, type=pathlib.Path)
    parser.add_argument("--wheelhouse", required=True, type=pathlib.Path)
    parser.add_argument("--lock", required=True, type=pathlib.Path)
    parser.add_argument("--manifest", required=True, type=pathlib.Path)
    prepare(parser.parse_args())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
