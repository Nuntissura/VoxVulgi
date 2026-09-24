#!/usr/bin/env python3
"""Build the two governed Windows pure wheels without executing sdist code.

This is deliberately not a general sdist builder.  It accepts only the exact
PyPI source archives and exact runtime members declared below.  setup.py,
pyproject hooks, PKG-INFO, and package code are never imported or executed.
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
import stat
import tarfile
import tempfile
import unicodedata
import zipfile
from dataclasses import dataclass
from typing import Callable, Iterable


SCHEMA = "voxvulgi.governed_pure_wheels.v1"
HELPER_VERSION = "1"
FIXED_ZIP_TIME = (1980, 1, 1, 0, 0, 0)
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


@dataclass(frozen=True)
class SourceMember:
    source: str
    output: str
    size: int
    sha256: str
    output_size: int | None = None
    output_sha256: str | None = None
    transform: str | None = None


@dataclass(frozen=True)
class Recipe:
    package: str
    version: str
    source_filename: str
    source_size: int
    source_sha256: str
    source_kind: str
    archive_member_count: int
    max_archive_bytes: int
    max_member_bytes: int
    members: tuple[SourceMember, ...]
    metadata: str
    entry_points: str | None

    @property
    def wheel_stem(self) -> str:
        return f"{self.package.replace('-', '_')}-{self.version}"

    @property
    def wheel_filename(self) -> str:
        return f"{self.wheel_stem}-py3-none-any.whl"

    @property
    def dist_info(self) -> str:
        return f"{self.wheel_stem}.dist-info"


WHISPER_MEMBERS = (
    SourceMember("openai-whisper-20231117/LICENSE", "openai_whisper-20231117.dist-info/LICENSE", 1063, "b5d65a59060e68c4ff940e1eddfa6f94b2d68fdf58ed7f4dd57721c997e35e9d"),
    SourceMember("openai-whisper-20231117/whisper/__init__.py", "whisper/__init__.py", 6901, "35969670c1c07ddfc515479dd4e6afcc21c722c2800b23355ec551f4d83081ca"),
    SourceMember("openai-whisper-20231117/whisper/__main__.py", "whisper/__main__.py", 35, "c519d586c3a6b639f64e59d2d6fd6b591a67ecd739f9727492911fc4ce4d545a"),
    SourceMember("openai-whisper-20231117/whisper/assets/gpt2.tiktoken", "whisper/assets/gpt2.tiktoken", 835554, "306cd27f03c1a714eca7108e03d66b7dc042abe8c258b44c199a7ed9838dd930"),
    SourceMember("openai-whisper-20231117/whisper/assets/mel_filters.npz", "whisper/assets/mel_filters.npz", 4271, "7450ae70723a5ef9d341e3cee628c7cb0177f36ce42c44b7ed2bf3325f0f6d4c"),
    SourceMember("openai-whisper-20231117/whisper/assets/multilingual.tiktoken", "whisper/assets/multilingual.tiktoken", 816730, "b34b360dbb493e781e479794586d661700670d65564001f23024971d1f2fa126"),
    SourceMember("openai-whisper-20231117/whisper/audio.py", "whisper/audio.py", 4932, "d93b25bda8d701630764f348d3023d1f6db4f4ea5e1708d556ed2b11d2b01df2"),
    SourceMember("openai-whisper-20231117/whisper/decoding.py", "whisper/decoding.py", 32155, "94084fedc5af74cabf8e1883a85552700057081f00ea669ad49435ecb29039db"),
    SourceMember("openai-whisper-20231117/whisper/model.py", "whisper/model.py", 10847, "17ffd2a7b8cdce2868550bb4144eb97a7ca93bbb56408192a21892bafc64480a"),
    SourceMember("openai-whisper-20231117/whisper/normalizers/__init__.py", "whisper/normalizers/__init__.py", 130, "f18fcdce4caee7f2e80e4c01ab0457d3a080a872a29489542c6d23c4b0bba572"),
    SourceMember("openai-whisper-20231117/whisper/normalizers/basic.py", "whisper/normalizers/basic.py", 1936, "ad567b3a0e1d03ccc0c35fc746fd6830c35290fe4ed7674eb61ce157ba3d380d"),
    SourceMember("openai-whisper-20231117/whisper/normalizers/english.json", "whisper/normalizers/english.json", 56128, "6607f948be9824d2e1b2fa2223cd94c06c45afa4e05ea0e3d5e1f2bdffde2465"),
    SourceMember("openai-whisper-20231117/whisper/normalizers/english.py", "whisper/normalizers/english.py", 20868, "2898e4672aeefc9f44cb4de38c115cdf286f5b35ae321433467a7674ce8841d0"),
    SourceMember("openai-whisper-20231117/whisper/timing.py", "whisper/timing.py", 12583, "24f9cd5b0fb0f4f4f9abbc93a70e3deb08a27aa3b556d94dfbfebad1201d3b31"),
    SourceMember("openai-whisper-20231117/whisper/tokenizer.py", "whisper/tokenizer.py", 12338, "3b48e361a7e95b4ec0356ca6d72bba635778aa10269153136ee7bc34cae30b85"),
    SourceMember("openai-whisper-20231117/whisper/transcribe.py", "whisper/transcribe.py", 22796, "f92867cb18eebcffba99937b9d082051e0d1b7361ec43e99e18817c3e1d19ec0"),
    SourceMember("openai-whisper-20231117/whisper/triton_ops.py", "whisper/triton_ops.py", 3474, "164c8317baac5a415615206e489e12bc7bdeb6c43ba4fca1bb20c626f8cdc957"),
    SourceMember("openai-whisper-20231117/whisper/utils.py", "whisper/utils.py", 11041, "faff52e95d927d6ad5671a7f5b903f87eff8816abd6a6c51a3ec37044dffd883"),
    SourceMember("openai-whisper-20231117/whisper/version.py", "whisper/version.py", 25, "190a8ec3bf4d187f43c613afd7f785740a3e89495ef13fb4b6a5e7aa8897fa74"),
)

WHISPER_METADATA = """Metadata-Version: 2.1
Name: openai-whisper
Version: 20231117
Summary: Robust Speech Recognition via Large-Scale Weak Supervision
License: MIT
Requires-Python: >=3.8
Requires-Dist: numba
Requires-Dist: numpy
Requires-Dist: torch
Requires-Dist: tqdm
Requires-Dist: more-itertools
Requires-Dist: tiktoken

"""

WGET_METADATA = """Metadata-Version: 2.1
Name: wget
Version: 3.2
Summary: Pure Python download utility
License: Public Domain

"""

OPENVOICE_METADATA = """Metadata-Version: 2.1
Name: MyShell-OpenVoice
Version: 0.0.0
Summary: Instant voice cloning by MyShell.
Home-page: https://github.com/myshell-ai/OpenVoice
Author: MyShell
Author-email: ethan@myshell.ai
License: MIT License
Requires-Python: >=3.9
Requires-Dist: torch==2.10.0
Requires-Dist: numpy==1.26.4
Requires-Dist: librosa==0.11.0
Requires-Dist: soundfile==0.13.1
Requires-Dist: eng_to_ipa==0.0.2
Requires-Dist: inflect==7.5.0
Requires-Dist: Unidecode==1.4.0
Requires-Dist: pypinyin==0.55.0
Requires-Dist: cn2an==0.5.23
Requires-Dist: jieba==0.42.1

"""

ENG_TO_IPA_METADATA = """Metadata-Version: 2.1
Name: eng_to_ipa
Version: 0.0.2
Summary: take English text and convert it to IPA
Author: ['mphilli', 'Mitchellpkt', 'CanadianCommander', 'timvancann']
License: UNKNOWN

"""

JIEBA_METADATA = """Metadata-Version: 2.1
Name: jieba
Version: 0.42.1
Summary: Chinese Words Segmentation Utilities
Home-page: https://github.com/fxsjy/jieba
Author: Sun, Junyi
Author-email: ccnusjy@gmail.com
License: MIT

"""

RECIPES = (
    Recipe(
        package="openai-whisper",
        version="20231117",
        source_filename="openai-whisper-20231117.tar.gz",
        source_size=798593,
        source_sha256="7af424181436f1800cc0b7d75cf40ede34e9ddf1ba4983a910832fcf4aade4a4",
        source_kind="tar.gz",
        archive_member_count=37,
        max_archive_bytes=3_000_000,
        max_member_bytes=1_000_000,
        members=WHISPER_MEMBERS,
        metadata=WHISPER_METADATA,
        entry_points="[console_scripts]\nwhisper=whisper.transcribe:cli\n",
    ),
    Recipe(
        package="wget",
        version="3.2",
        source_filename="wget-3.2.zip",
        source_size=10857,
        source_sha256="35e630eca2aa50ce998b9b1a127bb26b30dfee573702782aa982f875e3f16061",
        source_kind="zip",
        archive_member_count=4,
        max_archive_bytes=100_000,
        max_member_bytes=100_000,
        members=(
            SourceMember("wget-3.2/wget.py", "wget.py", 22355, "17e1e138d89005e6d0c487878f0489f9ec60d7f0c632135627e4efb26a0c115f"),
        ),
        metadata=WGET_METADATA,
        entry_points=None,
    ),
    Recipe(
        package="MyShell-OpenVoice",
        version="0.0.0",
        source_filename="OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23.tar.gz",
        source_size=3193160,
        source_sha256="a0bc7e5b2968ab7f8898d237b2b36f7de46710cbc3901f97dff29781eeae8abd",
        source_kind="tar.gz",
        archive_member_count=42,
        max_archive_bytes=4_000_000,
        max_member_bytes=4_000_000,
        members=(
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/LICENSE", "MyShell_OpenVoice-0.0.0.dist-info/LICENSE", 1057, "bb3541f421e6273d3a3e3dd5bdba2bd3b79a3c2eadca65cde9c62c2467903ac0"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/__init__.py", "openvoice/__init__.py", 0, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/api.py", "openvoice/api.py", 7823, "6830abaf9b0ea6023fe7e3df3d9a0a1fd9b73a7dbbc13f8a892da009052637b9", 7867, "00c870aea27bffdba2f3c9f0f93be03a719ef1f7b794d9a393587bc50cc83efb", "openvoice_disable_watermark_v1"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/attentions.py", "openvoice/attentions.py", 16360, "6a392057838e6051afc8beed43c2d1279d75de8c8b600552e7232ad930b356c3"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/commons.py", "openvoice/commons.py", 4956, "046943d265635c3d9c99195562e703564a26300981a7db5b87d4cd3fd180ae0d"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/mel_processing.py", "openvoice/mel_processing.py", 6098, "b8fd9924f8fb4c5b31031e04464c4a8145104e4bdfc3c29ecb6c485fa3b5c049"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/models.py", "openvoice/models.py", 16801, "54f9e8284a0e9a89c75cb3163d2694cf34d90a5d05cfbcbf12ccd4f0b1ba2dc8"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/modules.py", "openvoice/modules.py", 19010, "b2376049141934eb63bb4d905920c486a286bec65f0735b78ba52b2fb54564d0"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/text/__init__.py", "openvoice/text/__init__.py", 2757, "f04afd34eb0f2258eeb19a8bac80ce329eb2b2b4c0c13dc0c0502f6a752d19b5"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/text/cleaners.py", "openvoice/text/cleaners.py", 846, "ddd518de8e37112ee357475d355428d12ee3462eeb5152dfdaa16594c8313ba6"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/text/english.py", "openvoice/text/english.py", 5559, "f439b50c707a4a8dd383af3e992ead4ccabab43eb8c5ab515d4b2dfcdbd85f10"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/text/mandarin.py", "openvoice/text/mandarin.py", 7717, "147cb7c5db5cc3104660dd50df145435bb103c49209166df0c50a18706dcb077"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/text/symbols.py", "openvoice/text/symbols.py", 2588, "d957e98403192800ec9cad22bd973f2505ba0794bfd19a53df6f292608467f04"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/transforms.py", "openvoice/transforms.py", 7253, "e6798989a0d75cbec45e7e24e3697e1c4886b3664c6a80a452645ef1d2a93f9c"),
            SourceMember("OpenVoice-74a1d147b17a8c3092dd5430504bd83ef6c7eb23/openvoice/utils.py", "openvoice/utils.py", 5776, "87c1e5b375503e07098d7d21d5493a96f2a4f084893dade063f6aac5065638bd"),
        ),
        metadata=OPENVOICE_METADATA,
        entry_points=None,
    ),
    Recipe(
        package="eng_to_ipa",
        version="0.0.2",
        source_filename="eng_to_ipa-0.0.2.tar.gz",
        source_size=2808070,
        source_sha256="0e4fac8370b0ffeaf696193e971b3ff9bd3762e4d153c6d0d280147887e008b1",
        source_kind="tar.gz",
        archive_member_count=22,
        max_archive_bytes=9_186_674,
        max_member_bytes=9_186_674,
        members=(
            SourceMember("eng_to_ipa-0.0.2/eng_to_ipa/__init__.py", "eng_to_ipa/__init__.py", 180, "2ef06b7fb165b4364c3031205a1c8b69a011c6ef3c9ee072286b7972fe58571b"),
            SourceMember("eng_to_ipa-0.0.2/eng_to_ipa/resources/CMU_dict.db", "eng_to_ipa/resources/CMU_dict.db", 4657152, "1fa2b16d26169f50e8343c940cdcd2a2624a547096a89062752703d4190a95a9"),
            SourceMember("eng_to_ipa-0.0.2/eng_to_ipa/resources/CMU_dict.json", "eng_to_ipa/resources/CMU_dict.json", 4498493, "b0609f32b65f4d04466897a37fb55d3d5e877b65f3df46c145c3c2217b1ff55d"),
            SourceMember("eng_to_ipa-0.0.2/eng_to_ipa/resources/phones.json", "eng_to_ipa/resources/phones.json", 616, "98a613b148b511b4f18f9fab7e8a78006070daa9a3174b22cf18b50e8f43dc29"),
            SourceMember("eng_to_ipa-0.0.2/eng_to_ipa/rhymes.py", "eng_to_ipa/rhymes.py", 1484, "15834c7fd578d9259662c7bd4213acee5fe827b298f9a46323c9305df25af1f3"),
            SourceMember("eng_to_ipa-0.0.2/eng_to_ipa/stress.py", "eng_to_ipa/stress.py", 4956, "9b578e10d7cfd557ef4048e4fec40ae59b1f66f247c7fdede68a3beda0b1e4aa"),
            SourceMember("eng_to_ipa-0.0.2/eng_to_ipa/syllables.py", "eng_to_ipa/syllables.py", 1302, "0ddd6ea23debc0ac494b6e8677b180571020f9ba788683dfeb282249b850c4ad"),
            SourceMember("eng_to_ipa-0.0.2/eng_to_ipa/transcribe.py", "eng_to_ipa/transcribe.py", 8117, "9ae5db8321c810a6f3852fbd1f1f081cf46ade2341a3e0b0946fb3ce5a37019c"),
            SourceMember("eng_to_ipa-0.0.2/eng_to_ipa/transcriber.py", "eng_to_ipa/transcriber.py", 630, "5de5d0aadce0aee5384b854cd9670fb91372ba3d8cc5600d334059bfbeed026d"),
        ),
        metadata=ENG_TO_IPA_METADATA,
        entry_points=None,
    ),
    Recipe(
        package="jieba",
        version="0.42.1",
        source_filename="jieba-0.42.1.tar.gz",
        source_size=19214172,
        source_sha256="055ca12f62674fafed09427f176506079bc135638a14e23e25be909131928db2",
        source_kind="tar.gz",
        archive_member_count=84,
        max_archive_bytes=38_326_019,
        max_member_bytes=38_326_019,
        members=(
            SourceMember("jieba-0.42.1/jieba/__init__.py", "jieba/__init__.py", 19809, "b47cefcfd0bf683b0c708c1de0328df83be2c35264637c442106e79d739fb7f3"),
            SourceMember("jieba-0.42.1/jieba/_compat.py", "jieba/_compat.py", 2785, "ce079f41be19a25857607275336073703a3fa116c66472821a1f6f53472a6023"),
            SourceMember("jieba-0.42.1/jieba/dict.txt", "jieba/dict.txt", 5071852, "7197c3211ddd98962b036cdf40324d1ea2bfaa12bd028e68faa70111a88e12a8"),
            SourceMember("jieba-0.42.1/jieba/finalseg/__init__.py", "jieba/finalseg/__init__.py", 2659, "2168475bf522c0dae9d0a5e2bcb63c11b8c15931a622035cb8905d1e77e94f84"),
            SourceMember("jieba-0.42.1/jieba/finalseg/prob_start.py", "jieba/finalseg/prob_start.py", 93, "14c5706ced5cd3b42eb4873d4b88f7f52a7bdf80fbd767bc4423d361e20c5330"),
            SourceMember("jieba-0.42.1/jieba/finalseg/prob_trans.py", "jieba/finalseg/prob_trans.py", 241, "54dfbc252ed71480d4f0cdfdf516ecfbe44efd0f6c3c64b158e7039f2906c91b"),
            SourceMember("jieba-0.42.1/jieba/finalseg/prob_emit.py", "jieba/finalseg/prob_emit.py", 1321732, "27d46b1c9efe4dd148fde8be042a21be40e3562d0c7f1273f9de7abae12ebb8d"),
        ),
        metadata=JIEBA_METADATA,
        entry_points=None,
    ),
)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_path(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def normalized_safe_path(raw: str) -> str:
    if not raw or "\x00" in raw or "\\" in raw:
        raise ValueError(f"unsafe archive path: {raw!r}")
    value = unicodedata.normalize("NFC", raw)
    if value != raw or value.startswith(("/", "//")) or re.match(r"^[A-Za-z]:", value):
        raise ValueError(f"non-canonical archive path: {raw!r}")
    parts = value.split("/")
    if any(not part or part in {".", ".."} for part in parts):
        raise ValueError(f"unsafe archive path component: {raw!r}")
    for part in parts:
        if ":" in part or part.endswith((".", " ")):
            raise ValueError(f"Windows-unsafe archive path component: {raw!r}")
        base = part.split(".", 1)[0].casefold()
        if base in RESERVED_WINDOWS_NAMES:
            raise ValueError(f"reserved Windows archive path component: {raw!r}")
    return value


def validate_archive_names(names: Iterable[str]) -> None:
    seen: set[str] = set()
    for raw in names:
        name = normalized_safe_path(raw.rstrip("/"))
        key = name.casefold()
        if key in seen:
            raise ValueError(f"duplicate archive path after Windows normalization: {raw!r}")
        seen.add(key)


def read_tar_members(source_bytes: bytes, recipe: Recipe) -> dict[str, bytes]:
    accepted = {member.source: member for member in recipe.members}
    output: dict[str, bytes] = {}
    names: list[str] = []
    total = 0
    with tarfile.open(fileobj=io.BytesIO(source_bytes), mode="r:gz") as archive:
        expected_global_pax = (
            {"comment": "74a1d147b17a8c3092dd5430504bd83ef6c7eb23"}
            if recipe.package == "MyShell-OpenVoice"
            else {}
        )
        if archive.pax_headers != expected_global_pax:
            raise ValueError("global PAX headers drifted from the exact recipe")
        members = archive.getmembers()
        if len(members) != recipe.archive_member_count:
            raise ValueError(f"tar member count mismatch: {len(members)}")
        for item in members:
            names.append(item.name)
            if item.sparse:
                raise ValueError(f"sparse tar member is forbidden: {item.name}")
            if recipe.package == "MyShell-OpenVoice":
                pax_valid = item.pax_headers == expected_global_pax
            else:
                pax_valid = not (set(item.pax_headers) - {"mtime"}) and (
                    "mtime" not in item.pax_headers
                    or re.fullmatch(r"[0-9]+(?:\.[0-9]+)?", item.pax_headers["mtime"])
                    is not None
                )
            if not pax_valid:
                raise ValueError(f"unexpected PAX tar metadata: {item.name}")
            if not (item.isfile() or item.isdir()):
                raise ValueError(f"non-regular tar member is forbidden: {item.name}")
            if item.isfile():
                if item.size < 0 or item.size > recipe.max_member_bytes:
                    raise ValueError(f"tar member size is outside the recipe bound: {item.name}")
                total += item.size
                expected = accepted.get(item.name)
                if expected is not None:
                    stream = archive.extractfile(item)
                    if stream is None:
                        raise ValueError(f"could not stream governed tar member: {item.name}")
                    data = stream.read()
                    verify_source_member(expected, data)
                    output[item.name] = data
    validate_archive_names(names)
    if total > recipe.max_archive_bytes:
        raise ValueError("tar expanded byte bound exceeded")
    require_all_members(recipe, output)
    return output


def read_zip_members(source_bytes: bytes, recipe: Recipe) -> dict[str, bytes]:
    accepted = {member.source: member for member in recipe.members}
    output: dict[str, bytes] = {}
    total = 0
    with zipfile.ZipFile(io.BytesIO(source_bytes), "r") as archive:
        members = archive.infolist()
        if len(members) != recipe.archive_member_count:
            raise ValueError(f"zip member count mismatch: {len(members)}")
        validate_archive_names(member.filename for member in members)
        for item in members:
            if item.flag_bits & 0x1:
                raise ValueError(f"encrypted zip member is forbidden: {item.filename}")
            mode = (item.external_attr >> 16) & 0o170000
            if mode not in {0, stat.S_IFREG, stat.S_IFDIR}:
                raise ValueError(f"non-regular zip member is forbidden: {item.filename}")
            if item.file_size < 0 or item.file_size > recipe.max_member_bytes:
                raise ValueError(f"zip member size is outside the recipe bound: {item.filename}")
            if item.compress_size and item.file_size / item.compress_size > 100:
                raise ValueError(f"zip compression ratio is outside the recipe bound: {item.filename}")
            total += item.file_size
            expected = accepted.get(item.filename)
            if expected is not None:
                data = archive.read(item)
                verify_source_member(expected, data)
                output[item.filename] = data
    if total > recipe.max_archive_bytes:
        raise ValueError("zip expanded byte bound exceeded")
    require_all_members(recipe, output)
    return output


def verify_source_member(expected: SourceMember, data: bytes) -> None:
    if len(data) != expected.size or sha256_bytes(data) != expected.sha256:
        raise ValueError(f"governed source member drift: {expected.source}")


def require_all_members(recipe: Recipe, observed: dict[str, bytes]) -> None:
    expected = {member.source for member in recipe.members}
    if set(observed) != expected:
        raise ValueError(f"governed source member set mismatch for {recipe.package}")


def wheel_hash(data: bytes) -> str:
    return "sha256=" + base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode("ascii")


def governed_output_bytes(member: SourceMember, source: bytes) -> bytes:
    if member.transform is None:
        output = source
    elif member.transform == "openvoice_disable_watermark_v1":
        anchor_a = b"super().__init__(*args, **kwargs)"
        replacement_a = (
            b"enable_watermark = kwargs.pop('enable_watermark', True)\n"
            b"        super().__init__(*args, **kwargs)"
        )
        anchor_b = b"if kwargs.get('enable_watermark', True):"
        replacement_b = b"if enable_watermark:"
        if source.count(anchor_a) != 1 or source.count(anchor_b) != 1:
            raise ValueError("OpenVoice watermark transform anchors drifted")
        output = source.replace(anchor_a, replacement_a, 1).replace(anchor_b, replacement_b, 1)
    else:
        raise ValueError(f"unsupported governed source transform: {member.transform}")
    expected_size = member.output_size if member.output_size is not None else member.size
    expected_hash = member.output_sha256 if member.output_sha256 is not None else member.sha256
    if len(output) != expected_size or sha256_bytes(output) != expected_hash:
        raise ValueError(f"governed output member drift: {member.output}")
    if member.output.endswith(".py"):
        compile(output.decode("utf-8"), member.output, "exec", dont_inherit=True)
    return output


def zip_entry(name: str, data: bytes) -> tuple[zipfile.ZipInfo, bytes]:
    normalized_safe_path(name)
    info = zipfile.ZipInfo(name, FIXED_ZIP_TIME)
    info.compress_type = zipfile.ZIP_STORED
    info.create_system = 3
    info.external_attr = (stat.S_IFREG | 0o644) << 16
    info.flag_bits |= 0x800
    return info, data


def expected_wheel_metadata() -> bytes:
    return (
        "Wheel-Version: 1.0\n"
        f"Generator: voxvulgi-governed-pure-wheel/{HELPER_VERSION}\n"
        "Root-Is-Purelib: true\n"
        "Tag: py3-none-any\n\n"
    ).encode("utf-8")


def build_wheel_bytes(recipe: Recipe, source: dict[str, bytes]) -> bytes:
    entries: dict[str, bytes] = {}
    for member in recipe.members:
        entries[member.output] = governed_output_bytes(member, source[member.source])
    entries[f"{recipe.dist_info}/METADATA"] = recipe.metadata.encode("utf-8")
    entries[f"{recipe.dist_info}/WHEEL"] = expected_wheel_metadata()
    if recipe.entry_points is not None:
        entries[f"{recipe.dist_info}/entry_points.txt"] = recipe.entry_points.encode("utf-8")
    record_path = f"{recipe.dist_info}/RECORD"
    rows = []
    for name in sorted(entries):
        data = entries[name]
        rows.append((name, wheel_hash(data), str(len(data))))
    rows.append((record_path, "", ""))
    record = io.StringIO(newline="")
    writer = csv.writer(record, lineterminator="\n")
    writer.writerows(rows)
    entries[record_path] = record.getvalue().encode("utf-8")
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", compression=zipfile.ZIP_STORED) as wheel:
        for name in sorted(entries):
            info, data = zip_entry(name, entries[name])
            wheel.writestr(info, data, compress_type=zipfile.ZIP_STORED)
    result = buffer.getvalue()
    validate_wheel_bytes(recipe, result)
    return result


def validate_wheel_bytes(recipe: Recipe, wheel_bytes: bytes) -> None:
    with zipfile.ZipFile(io.BytesIO(wheel_bytes), "r") as wheel:
        infos = wheel.infolist()
        names = [item.filename for item in infos]
        validate_archive_names(names)
        if names != sorted(names) or len(names) != len(set(name.casefold() for name in names)):
            raise ValueError("wheel member order or uniqueness mismatch")
        expected_runtime = {member.output for member in recipe.members}
        expected_metadata = {
            f"{recipe.dist_info}/METADATA",
            f"{recipe.dist_info}/WHEEL",
            f"{recipe.dist_info}/RECORD",
        }
        if recipe.entry_points is not None:
            expected_metadata.add(f"{recipe.dist_info}/entry_points.txt")
        if set(names) != expected_runtime | expected_metadata:
            raise ValueError("wheel contains missing or extra entries")
        for member in recipe.members:
            data = wheel.read(member.output)
            expected_size = member.output_size if member.output_size is not None else member.size
            expected_hash = member.output_sha256 if member.output_sha256 is not None else member.sha256
            if len(data) != expected_size or sha256_bytes(data) != expected_hash:
                raise ValueError(f"wheel runtime member drift: {member.output}")
        for item in infos:
            if item.date_time != FIXED_ZIP_TIME or item.flag_bits & 0x1:
                raise ValueError(f"wheel entry metadata drift: {item.filename}")
            if item.compress_type != zipfile.ZIP_STORED or item.extra or item.comment:
                raise ValueError(f"wheel entry storage metadata drift: {item.filename}")
            if item.create_system != 3:
                raise ValueError(f"wheel entry creator-system drift: {item.filename}")
            mode = (item.external_attr >> 16) & 0o777
            file_type = (item.external_attr >> 16) & 0o170000
            if mode != 0o644 or file_type != stat.S_IFREG:
                raise ValueError(f"wheel entry mode drift: {item.filename}")
            if item.filename.endswith(".pth") or ".data/scripts/" in item.filename:
                raise ValueError(f"executable wheel hook is forbidden: {item.filename}")
        metadata = wheel.read(f"{recipe.dist_info}/METADATA").decode("utf-8")
        if metadata != recipe.metadata:
            raise ValueError("wheel METADATA drift")
        if wheel.read(f"{recipe.dist_info}/WHEEL") != expected_wheel_metadata():
            raise ValueError("wheel WHEEL contract drift")
        if recipe.entry_points is not None and wheel.read(
            f"{recipe.dist_info}/entry_points.txt"
        ) != recipe.entry_points.encode("utf-8"):
            raise ValueError("wheel entry-points contract drift")
        record_name = f"{recipe.dist_info}/RECORD"
        rows = list(csv.reader(io.StringIO(wheel.read(record_name).decode("utf-8"))))
        if len(rows) != len(names) or any(len(row) != 3 for row in rows):
            raise ValueError("wheel RECORD row count/shape mismatch")
        observed: set[str] = set()
        for name, digest, size in rows:
            if name in observed:
                raise ValueError(f"duplicate wheel RECORD entry: {name}")
            observed.add(name)
            if name == record_name:
                if digest or size:
                    raise ValueError("wheel RECORD self row must be blank")
                continue
            data = wheel.read(name)
            if digest != wheel_hash(data) or size != str(len(data)):
                raise ValueError(f"wheel RECORD hash/size mismatch: {name}")
        if observed != set(names):
            raise ValueError("wheel RECORD membership mismatch")


def load_recipe_source(recipe: Recipe, source_dir: pathlib.Path) -> tuple[pathlib.Path, dict[str, bytes]]:
    source_path = source_dir / recipe.source_filename
    if not source_path.is_file() or source_path.is_symlink():
        raise ValueError(f"governed source is missing or linked: {source_path}")
    source_bytes = source_path.read_bytes()
    if len(source_bytes) != recipe.source_size or sha256_bytes(source_bytes) != recipe.source_sha256:
        raise ValueError(f"governed source bytes drifted: {source_path}")
    readers: dict[str, Callable[[bytes, Recipe], dict[str, bytes]]] = {
        "tar.gz": read_tar_members,
        "zip": read_zip_members,
    }
    return source_path, readers[recipe.source_kind](source_bytes, recipe)


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


def build(source_dir: pathlib.Path, output_dir: pathlib.Path) -> None:
    if output_dir.exists() or output_dir.is_symlink():
        raise ValueError(f"output directory must be absent: {output_dir}")
    output_parent = output_dir.parent
    output_parent.mkdir(parents=True, exist_ok=True)
    parent_stat = output_parent.stat()
    if getattr(parent_stat, "st_file_attributes", 0) & 0x400:
        raise ValueError(f"output parent is a Windows reparse point: {output_parent}")
    producer_lock = output_parent / f".{output_dir.name}.producer.lock"
    lock_descriptor = os.open(producer_lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    lock_identity = os.fstat(lock_descriptor)
    try:
        os.write(lock_descriptor, f"pid={os.getpid()}\n".encode("ascii"))
        os.fsync(lock_descriptor)
    except BaseException:
        os.close(lock_descriptor)
        try:
            producer_lock.unlink()
        except OSError:
            pass
        raise
    staging: pathlib.Path | None = None
    helper_path = pathlib.Path(__file__).resolve()
    manifest_packages = []
    expected_outputs: set[str] = {"governed_pure_wheels.manifest.json"}
    try:
        staging = pathlib.Path(
            tempfile.mkdtemp(prefix=f".{output_dir.name}.", suffix=".staging", dir=output_parent)
        )
        for recipe in RECIPES:
            source_path, source_members = load_recipe_source(recipe, source_dir)
            first = build_wheel_bytes(recipe, source_members)
            second = build_wheel_bytes(recipe, source_members)
            if first != second:
                raise ValueError(f"non-reproducible governed wheel: {recipe.wheel_filename}")
            wheel_path = staging / recipe.wheel_filename
            atomic_write(wheel_path, first)
            validate_wheel_bytes(recipe, wheel_path.read_bytes())
            expected_outputs.add(recipe.wheel_filename)
            manifest_packages.append(
                {
                    "name": recipe.package,
                    "version": recipe.version,
                    "source": {
                        "filename": recipe.source_filename,
                        "bytes": recipe.source_size,
                        "sha256": recipe.source_sha256,
                    },
                    "members": [
                        {
                            "source": member.source,
                            "output": member.output,
                            "bytes": member.size,
                            "sha256": member.sha256,
                            "output_bytes": member.output_size if member.output_size is not None else member.size,
                            "output_sha256": member.output_sha256 if member.output_sha256 is not None else member.sha256,
                            "transform": member.transform,
                        }
                        for member in recipe.members
                    ],
                    "metadata_sha256": sha256_bytes(recipe.metadata.encode("utf-8")),
                    "requires_dist": [
                        line.removeprefix("Requires-Dist: ")
                        for line in recipe.metadata.splitlines()
                        if line.startswith("Requires-Dist: ")
                    ],
                    "wheel": {
                        "filename": recipe.wheel_filename,
                        "bytes": len(first),
                        "sha256": sha256_bytes(first),
                        "tag": "py3-none-any",
                        "root_is_purelib": True,
                    },
                },
            )
        manifest = {
            "schema": SCHEMA,
            "helper": {
                "version": HELPER_VERSION,
                "filename": helper_path.name,
                "bytes": helper_path.stat().st_size,
                "sha256": sha256_path(helper_path),
            },
            "platform_contract": "windows_x64_cp311",
            "build_contract": "static_no_sdist_execution_deterministic_zip_v1",
            "packages": manifest_packages,
        }
        manifest_bytes = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode("utf-8")
        atomic_write(staging / "governed_pure_wheels.manifest.json", manifest_bytes)
        observed_outputs = {path.name for path in staging.iterdir()}
        if observed_outputs != expected_outputs or any(not path.is_file() or path.is_symlink() for path in staging.iterdir()):
            raise ValueError(f"governed output exact-set mismatch: {sorted(observed_outputs)}")
        if output_dir.exists() or output_dir.is_symlink():
            raise ValueError(f"output directory appeared during production: {output_dir}")
        os.rename(staging, output_dir)
    except BaseException:
        import shutil

        if staging is not None:
            shutil.rmtree(staging, ignore_errors=True)
        raise
    finally:
        os.close(lock_descriptor)
        try:
            current = producer_lock.stat(follow_symlinks=False)
            if (current.st_dev, current.st_ino) == (lock_identity.st_dev, lock_identity.st_ino):
                producer_lock.unlink()
        except OSError:
            pass


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-dir", required=True, type=pathlib.Path)
    parser.add_argument("--output-dir", required=True, type=pathlib.Path)
    args = parser.parse_args()
    build(args.source_dir.resolve(), args.output_dir.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
