#!/usr/bin/env python3
"""VoxVulgi render wrapper for CosyVoice 2 zero-shot cross-lingual voice cloning.

Routed through the standard voice-preserving dub job, so it consumes the SAME
request format and emits the SAME report schema as the Kokoro+OpenVoice path
(``tts_voice_preserving_v1.py``). That lets the dub job reuse its manifest +
separation -> mix -> mux -> subtitle follow-up unchanged.

Invocation (matches the dub job's spawn):
  python voxvulgi_cosyvoice_render.py \
    --request request.json   # JSON LIST of segments (index, speaker, text,
                             #   out_path, base_out_path, render_mode,
                             #   tts_voice_profile_path[s], start_ms, end_ms)
    --report  report.json    # VoiceCloneReport (see jobs.rs VoiceCloneReport)
    --model-dir <pretrained_models>   # parent of CosyVoice2-0.5B

Hardening (per the audit): NO silent-failure fallback — a clone-intent segment
that fails records a real error and leaves no audio (the run reports the failure
instead of papering over it with silence); model/reference problems fail loudly.
"""

import argparse
import hashlib
import json
import os
import stat
import sys

sys.dont_write_bytecode = True
import threading
import time
import traceback


# The public render path is local-only. Apply the network-silent environment before
# importing torch, Transformers, ModelScope, or any CosyVoice module so neither warmup
# nor a job can turn a missing bundled byte into an implicit download.
for _name in (
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
):
    os.environ.pop(_name, None)
os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
os.environ["HF_DATASETS_OFFLINE"] = "1"

MODEL_MANIFEST_SHA256 = "3730f28cf1d7c31e663fb1823b4d1856aba04be7fda68c95f0536d9ea47c5374"
MODEL_REPO = "FunAudioLLM/CosyVoice2-0.5B"
MODEL_REVISION = "eec1ae6c79877dbd9379285cf8789c9e0879293d"
WETEXT_REPO = "pengzhendong/wetext"
WETEXT_REVISION = "b04bc07588601f7619b20efbb01cd1fa7278ccbc"
_REPARSE_ATTRIBUTE = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)


def _is_reparse(file_stat):
    return bool(getattr(file_stat, "st_file_attributes", 0) & _REPARSE_ATTRIBUTE)


def _file_identity(file_stat):
    return (int(file_stat.st_dev), int(file_stat.st_ino))


def _assert_plain_ancestor_chain(path):
    current = os.path.abspath(path)
    while True:
        parent = os.path.dirname(current)
        if parent == current:
            break
        current = parent
        current_stat = os.lstat(current)
        if not stat.S_ISDIR(current_stat.st_mode) or _is_reparse(current_stat):
            raise SystemExit(
                f"cosyvoice integrity: managed ancestor is linked/reparse-backed: {current}"
            )


def _hash_exact_regular_file(path, expected_bytes, expected_sha256):
    path_before = os.lstat(path)
    if (
        not stat.S_ISREG(path_before.st_mode)
        or _is_reparse(path_before)
        or int(path_before.st_nlink) != 1
        or int(path_before.st_size) != int(expected_bytes)
    ):
        raise SystemExit(f"cosyvoice integrity: unsafe or wrong-sized model file: {path}")
    digest = hashlib.sha256()
    with open(path, "rb", buffering=0) as stream:
        opened = os.fstat(stream.fileno())
        if (
            _file_identity(opened) != _file_identity(path_before)
            or int(opened.st_nlink) != 1
            or int(opened.st_size) != int(expected_bytes)
            or _is_reparse(opened)
        ):
            raise SystemExit(f"cosyvoice integrity: model pathname changed before hashing: {path}")
        while True:
            chunk = stream.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
        opened_after = os.fstat(stream.fileno())
    path_after = os.lstat(path)
    with open(path, "rb", buffering=0) as reopened_stream:
        reopened = os.fstat(reopened_stream.fileno())
    if (
        _file_identity(opened) != _file_identity(opened_after)
        or _file_identity(opened) != _file_identity(path_after)
        or _file_identity(opened) != _file_identity(reopened)
        or int(opened_after.st_nlink) != 1
        or int(path_after.st_nlink) != 1
        or int(reopened.st_nlink) != 1
        or int(path_after.st_size) != int(expected_bytes)
        or int(reopened.st_size) != int(expected_bytes)
        or _is_reparse(path_after)
        or _is_reparse(reopened)
        or digest.hexdigest() != expected_sha256
    ):
        raise SystemExit(f"cosyvoice integrity: model file identity mismatch: {path}")


def _verify_exact_tree(root, source):
    _assert_plain_ancestor_chain(root)
    root_before = os.lstat(root)
    if not stat.S_ISDIR(root_before.st_mode) or _is_reparse(root_before):
        raise SystemExit(f"cosyvoice integrity: unsafe model root: {root}")
    expected_files = {entry["path"]: entry for entry in source["files"]}
    expected_dirs = set(source.get("directories") or [])
    for relative in expected_files:
        parts = relative.split("/")
        expected_dirs.update("/".join(parts[:index]) for index in range(1, len(parts)))

    observed_files = set()
    observed_dirs = set()

    def visit(directory, relative_parent=""):
        with os.scandir(directory) as iterator:
            entries = sorted(iterator, key=lambda entry: entry.name)
        for entry in entries:
            if not entry.name.isascii() or any(value in entry.name for value in ("/", "\\", ":")):
                raise SystemExit(f"cosyvoice integrity: unsafe model entry name: {entry.name!r}")
            relative = entry.name if not relative_parent else f"{relative_parent}/{entry.name}"
            entry_stat = os.lstat(entry.path)
            if _is_reparse(entry_stat) or stat.S_ISLNK(entry_stat.st_mode):
                raise SystemExit(f"cosyvoice integrity: linked/reparse model entry: {relative}")
            if stat.S_ISDIR(entry_stat.st_mode):
                if relative not in expected_dirs or relative in observed_dirs:
                    raise SystemExit(f"cosyvoice integrity: unexpected model directory: {relative}")
                observed_dirs.add(relative)
                visit(entry.path, relative)
            elif stat.S_ISREG(entry_stat.st_mode):
                expected = expected_files.get(relative)
                if expected is None or relative in observed_files:
                    raise SystemExit(f"cosyvoice integrity: unexpected model file: {relative}")
                observed_files.add(relative)
                _hash_exact_regular_file(entry.path, expected["bytes"], expected["sha256"])
            else:
                raise SystemExit(f"cosyvoice integrity: unsupported model entry: {relative}")

    visit(root)
    root_after = os.lstat(root)
    if (
        _file_identity(root_before) != _file_identity(root_after)
        or _is_reparse(root_after)
        or observed_files != set(expected_files)
        or observed_dirs != expected_dirs
    ):
        raise SystemExit(f"cosyvoice integrity: incomplete or changed model tree: {root}")


def _contains_forbidden_model_key(value):
    if isinstance(value, dict):
        return any(
            key in {"auto_map", "_attn_implementation_internal", "trust_remote_code"}
            or _contains_forbidden_model_key(child)
            for key, child in value.items()
        )
    if isinstance(value, list):
        return any(_contains_forbidden_model_key(child) for child in value)
    return False


def _verify_exact_payload(model_parent):
    backend_root = os.path.dirname(os.path.abspath(__file__))
    manifest_path = os.path.join(backend_root, "cosyvoice_model_manifest.json")
    with open(manifest_path, "rb") as stream:
        manifest_bytes = stream.read()
    if hashlib.sha256(manifest_bytes).hexdigest() != MODEL_MANIFEST_SHA256:
        raise SystemExit("cosyvoice integrity: governed model manifest identity mismatch")
    manifest = json.loads(manifest_bytes)
    if (
        manifest.get("schema") != "voxvulgi.cosyvoice_model_manifest.v1"
        or manifest["cosyvoice"].get("provider") != "huggingface"
        or manifest["cosyvoice"].get("repo") != MODEL_REPO
        or manifest["cosyvoice"].get("revision") != MODEL_REVISION
        or len(manifest["cosyvoice"].get("files") or []) != 19
        or manifest["wetext"].get("provider") != "modelscope"
        or manifest["wetext"].get("repo") != WETEXT_REPO
        or manifest["wetext"].get("revision") != WETEXT_REVISION
        or len(manifest["wetext"].get("files") or []) != 26
        or len(manifest["wetext"].get("directories") or []) != 9
    ):
        raise SystemExit("cosyvoice integrity: governed model manifest contract mismatch")
    model_root = os.path.join(os.path.abspath(model_parent), "CosyVoice2-0.5B")
    wetext_root = os.path.join(backend_root, "wetext")
    _verify_exact_tree(model_root, manifest["cosyvoice"])
    _verify_exact_tree(wetext_root, manifest["wetext"])
    with open(os.path.join(model_root, "CosyVoice-BlankEN", "config.json"), "rb") as stream:
        blank_config = json.load(stream)
    if _contains_forbidden_model_key(blank_config):
        raise SystemExit("cosyvoice integrity: BlankEN config requests dynamic/remote model code")
    print("cosyvoice_exact_payload_verified", flush=True)

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(
    0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "third_party", "Matcha-TTS")
)

MAX_REFERENCE_SECONDS = 30.0  # CosyVoice frontend hard-asserts ref <= 30 s.

# WP-0262: the CosyVoice class import (`from cosyvoice.cli.cosyvoice import ...`) has
# been observed to take >150 s on a cold venv. Left un-instrumented it silently eats
# the dub job's timeout budget and the run ends with no audio and no diagnosis of
# WHERE it stalled. `IMPORT_WARN_EVERY_SECS` heartbeats show the import is still
# progressing (not deadlocked); `IMPORT_HARD_LIMIT_SECS` is a bounded ceiling that
# fails LOUDLY with a clear message so a slow/hung import surfaces as an explicit
# error instead of a mystery timeout. Override via env for slow disks.
IMPORT_WARN_EVERY_SECS = float(os.environ.get("VOXVULGI_COSYVOICE_IMPORT_WARN_SECS", "15"))
IMPORT_HARD_LIMIT_SECS = float(os.environ.get("VOXVULGI_COSYVOICE_IMPORT_LIMIT_SECS", "300"))


def _install_offline_wetext_resolver():
    """Resolve the wetext normalizer from the managed app-local asset directory.

    wetext 0.0.4 calls ``modelscope.snapshot_download`` while its Normalizer is
    constructed, even when a complete ModelScope cache already exists. The public
    VoxVulgi path is offline, so intercept that one known repository before importing
    CosyVoice and refuse every unexpected ModelScope lookup. This keeps the upstream
    frontend unchanged while making network silence an execution-boundary property.
    """
    model_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "wetext")
    required = (
        os.path.join("en", "tn", "tagger.fst"),
        os.path.join("en", "tn", "verbalizer.fst"),
        os.path.join("zh", "tn", "tagger.fst"),
        os.path.join("zh", "tn", "verbalizer.fst"),
    )
    missing = [
        rel
        for rel in required
        if not os.path.isfile(os.path.join(model_dir, rel))
        or os.path.getsize(os.path.join(model_dir, rel)) <= 0
    ]
    if missing:
        raise SystemExit(
            "cosyvoice render: managed wetext assets are incomplete under "
            f"{model_dir}: {', '.join(missing)}. Repair the CosyVoice 2 pack in Diagnostics."
        )

    import modelscope

    def _offline_snapshot_download(model_id=None, *args, **kwargs):
        requested = str(model_id or kwargs.get("repo_id") or "").strip()
        if requested == "pengzhendong/wetext":
            return model_dir
        raise RuntimeError(
            f"CosyVoice attempted an unmanaged ModelScope lookup while offline: {requested or '<empty>'}"
        )

    modelscope.snapshot_download = _offline_snapshot_download


def _instrumented_import_cosyvoice():
    """Import the CosyVoice class with a watchdog that logs progress and enforces a
    bounded ceiling.

    Returns the imported class (AutoModel when available, else CosyVoice2). Runs the
    (potentially minutes-long, GIL-releasing C-extension-heavy) import on a worker
    thread so a watchdog on the main thread can heartbeat elapsed time and abort with
    a clear, loud error if the import exceeds ``IMPORT_HARD_LIMIT_SECS``. Without this
    the import can silently consume the whole job timeout with no clue where it hung.
    """
    result = {}

    _install_offline_wetext_resolver()

    def _do_import():
        t0 = time.monotonic()
        try:
            # Prefer AutoModel (the render path's constructor); fall back to CosyVoice2.
            try:
                from cosyvoice.cli.cosyvoice import AutoModel as _cls  # noqa: F401
                result["name"] = "AutoModel"
            except Exception:  # noqa: BLE001 - fall back to the concrete class
                from cosyvoice.cli.cosyvoice import CosyVoice2 as _cls  # noqa: F401
                result["name"] = "CosyVoice2"
            result["cls"] = _cls
        except BaseException as exc:  # noqa: BLE001 - propagate the real import error
            result["error"] = exc
            result["traceback"] = traceback.format_exc()
        finally:
            result["elapsed"] = time.monotonic() - t0

    start = time.monotonic()
    print(
        f"[cosyvoice] importing CosyVoice class "
        f"(warn every {IMPORT_WARN_EVERY_SECS:.0f}s, hard limit {IMPORT_HARD_LIMIT_SECS:.0f}s)...",
        flush=True,
    )
    worker = threading.Thread(target=_do_import, name="cosyvoice-import", daemon=True)
    worker.start()

    next_warn = IMPORT_WARN_EVERY_SECS
    while worker.is_alive():
        worker.join(timeout=1.0)
        elapsed = time.monotonic() - start
        if elapsed >= next_warn and worker.is_alive():
            print(
                f"[cosyvoice] still importing CosyVoice class after {elapsed:.0f}s "
                f"(this import has been observed to take >150s on a cold venv; "
                f"aborting at {IMPORT_HARD_LIMIT_SECS:.0f}s)...",
                flush=True,
            )
            next_warn += IMPORT_WARN_EVERY_SECS
        if elapsed > IMPORT_HARD_LIMIT_SECS and worker.is_alive():
            # Loud, bounded failure. The daemon worker is abandoned (a hung native
            # import cannot be safely interrupted), and we exit non-zero so the Rust
            # side records a real error instead of a silent job-timeout.
            raise SystemExit(
                f"cosyvoice render: CosyVoice class import exceeded {IMPORT_HARD_LIMIT_SECS:.0f}s "
                f"and appears stalled inside `from cosyvoice.cli.cosyvoice import ...`. This is a "
                f"dependency/environment stall (WP-0262), not a job-content problem. Rebuild the "
                f"CosyVoice venv and re-run the install warmup to validate the import path."
            )

    if "error" in result:
        print(result.get("traceback", ""), flush=True)
        raise SystemExit(
            f"cosyvoice render: CosyVoice class import failed after "
            f"{result.get('elapsed', 0.0):.0f}s: {result['error']}"
        )
    print(
        f"[cosyvoice] imported {result['name']} in {result.get('elapsed', 0.0):.1f}s",
        flush=True,
    )
    return result["cls"]


def reference_duration_seconds(path):
    import soundfile as sf

    info = sf.info(path)
    if not info.samplerate:
        return None
    return float(info.frames) / float(info.samplerate)


def save_generated_wav(path, audio, sample_rate):
    """Write genuine channels-by-samples model output with the bundled SoundFile."""
    import soundfile
    import torch

    samples = audio.detach().to(device="cpu", dtype=torch.float32)
    if samples.ndim != 2 or samples.shape[0] == 0 or samples.shape[1] == 0:
        raise RuntimeError("CosyVoice generated empty or invalid audio dimensions")
    if not bool(torch.isfinite(samples).all()):
        raise RuntimeError("CosyVoice generated nonfinite audio")
    if bool((samples < -1.0).any()) or bool((samples > 32767.0 / 32768.0).any()):
        raise RuntimeError("CosyVoice generated audio exceeds the PCM16 range")
    if int(sample_rate) <= 0:
        raise RuntimeError("CosyVoice generated invalid sample rate")
    soundfile.write(path, samples.transpose(0, 1).numpy(), int(sample_rate),
                    format="WAV", subtype="PCM_16")


def normalize_cpu_llm_dtype(cosyvoice):
    """Keep the CPU-only CosyVoice LLM graph consistent with its FP32 embeddings."""
    model = cosyvoice.model

    def counts():
        result = {}
        for parameter in model.llm.parameters():
            if not parameter.is_floating_point():
                continue
            key = f"{parameter.dtype}@{parameter.device}"
            if key not in result and len(result) >= 7:
                key = "other"
            result[key] = result.get(key, 0) + 1
        return result

    before = counts()
    applied = model.device.type == "cpu"
    if applied:
        model.llm.float()
    return {"cpu_float32_applied": applied, "floating_parameters_before": before,
            "floating_parameters_after": counts()}


def failed_segment_diagnostics(seg, ref_path, chunks):
    """Bounded failure evidence; no transcript, new dependency, or fallback."""
    import wave

    result = {
        "traceback": traceback.format_exc()[-16384:],
        "text_characters": len(str(seg.get("text") or "")),
        "start_ms": seg.get("start_ms"),
        "end_ms": seg.get("end_ms"),
        "chunk_shapes": [],
    }
    try:
        result["chunk_shapes"] = [list(chunk.shape) for chunk in chunks[:8]]
    except Exception as diagnostic_error:
        result["chunk_probe_error"] = str(diagnostic_error)[:512]
    try:
        with wave.open(ref_path, "rb") as reference:
            result["reference"] = {
                "frames": reference.getnframes(),
                "sample_rate": reference.getframerate(),
                "channels": reference.getnchannels(),
                "sample_width_bytes": reference.getsampwidth(),
            }
    except Exception as diagnostic_error:
        result["reference_probe_error"] = str(diagnostic_error)[:512]
    return result


def pick_reference(seg):
    profiles = seg.get("tts_voice_profile_paths") or []
    if not isinstance(profiles, list):
        profiles = []
    for candidate in profiles:
        candidate = str(candidate or "").strip()
        if candidate and os.path.isfile(candidate):
            return candidate
    single = str(seg.get("tts_voice_profile_path") or "").strip()
    if single and os.path.isfile(single):
        return single
    return None


def run_warmup(model_dir):
    """WP-0262 bounded/instrumented warmup: import the CosyVoice class (loudly, with a
    watchdog), construct the model from the LOCAL dir (offline-by-design), and run one
    tiny synth. Prints per-stage elapsed timings so a slow import/model-load surfaces
    exactly WHERE it stalled instead of silently exceeding the caller's timeout. Exits
    non-zero on any stall/failure so the Rust install step records a real error.
    """
    model_path = os.path.join(model_dir, "CosyVoice2-0.5B")
    if not os.path.isdir(model_path):
        raise SystemExit(
            f"cosyvoice warmup: model dir not found: {model_path}. Install the CosyVoice pack first."
        )
    import torch  # noqa: F401
    import torchaudio  # noqa: F401

    cosyvoice_cls = _instrumented_import_cosyvoice()

    print(f"[cosyvoice] warmup: loading model from {model_path}", flush=True)
    _t0 = time.monotonic()
    cosyvoice = cosyvoice_cls(model_dir=model_path)
    dtype_diagnostics = normalize_cpu_llm_dtype(cosyvoice)
    print(f"[cosyvoice] llm_dtype {json.dumps(dtype_diagnostics, sort_keys=True)}", flush=True)
    print(f"[cosyvoice] warmup: model loaded in {time.monotonic() - _t0:.1f}s", flush=True)

    ref = os.path.join(os.path.dirname(os.path.abspath(__file__)), "asset", "zero_shot_prompt.wav")
    if not os.path.isfile(ref):
        raise SystemExit(f"cosyvoice warmup: reference prompt missing: {ref}")
    _t1 = time.monotonic()
    res = list(cosyvoice.inference_cross_lingual("<|en|>warmup.", ref, stream=False))
    if not (res and "tts_speech" in res[0]):
        raise SystemExit("cosyvoice warmup: produced no audio")
    print(
        f"[cosyvoice] warmup: synth ok in {time.monotonic() - _t1:.1f}s", flush=True
    )
    print("cosyvoice_warmup_ok", flush=True)


def main():
    parser = argparse.ArgumentParser(description="VoxVulgi CosyVoice render wrapper")
    parser.add_argument("--request", help="Path to request JSON (list of segments)")
    parser.add_argument("--report", help="Path to write VoiceCloneReport JSON")
    parser.add_argument("--model-dir", required=True, help="Parent dir containing CosyVoice2-0.5B")
    parser.add_argument("--backend", default="cosyvoice")
    parser.add_argument("--track", default="")
    parser.add_argument(
        "--warmup",
        action="store_true",
        help="WP-0262: run the bounded/instrumented import+model+synth warmup, then exit.",
    )
    args = parser.parse_args()

    # This is the wrapper's own pre-import gate. Rust also validates the same embedded
    # manifest before spawning us; keeping the gate here prevents a direct invocation
    # from weakening readiness to a handful of presence checks.
    _verify_exact_payload(args.model_dir)

    if args.warmup:
        run_warmup(args.model_dir)
        return

    if not args.request or not args.report:
        raise SystemExit("cosyvoice render: --request and --report are required (unless --warmup)")

    with open(args.request, "r", encoding="utf-8") as f:
        items = json.load(f)
    if not isinstance(items, list):
        raise SystemExit("cosyvoice render: --request must be a JSON list of segments")

    # Fail loudly (offline-by-design): the model must be present locally; never
    # try to resolve a remote id at job time.
    model_path = os.path.join(args.model_dir, "CosyVoice2-0.5B")
    if not os.path.isdir(model_path):
        raise SystemExit(
            f"cosyvoice render: model dir not found: {model_path}. Install the CosyVoice pack first."
        )

    import torch
    import torchaudio

    # WP-0262: bounded, instrumented import of the CosyVoice class. A slow/hung import
    # now fails LOUDLY with the stall location instead of silently eating the timeout.
    cosyvoice_cls = _instrumented_import_cosyvoice()

    print(f"[cosyvoice] loading model from {model_path}", flush=True)
    _load_t0 = time.monotonic()
    cosyvoice = cosyvoice_cls(model_dir=model_path)
    dtype_diagnostics = normalize_cpu_llm_dtype(cosyvoice)
    print(f"[cosyvoice] llm_dtype {json.dumps(dtype_diagnostics, sort_keys=True)}", flush=True)
    sample_rate = int(cosyvoice.sample_rate)
    print(
        f"[cosyvoice] model loaded in {time.monotonic() - _load_t0:.1f}s; "
        f"sample_rate={sample_rate}",
        flush=True,
    )

    segments = []
    converted_ok = 0
    clone_requested = 0
    clone_fallback = 0
    standard_tts_segments = 0

    for seg in items:
        idx = seg.get("index")
        speaker = (seg.get("speaker") or "").strip()
        text = (seg.get("text") or "").strip()
        out_path = (seg.get("out_path") or "").strip()
        base_out_path = (seg.get("base_out_path") or "").strip()
        render_mode = (seg.get("render_mode") or "").strip()
        if not text or not out_path:
            continue

        intent = "standard_tts" if render_mode == "standard_tts" else "clone"
        if intent == "clone":
            clone_requested += 1
        else:
            standard_tts_segments += 1

        rec = {
            "index": idx,
            "speaker": speaker or None,
            "text_len": len(text),
            "base_out_path": base_out_path or out_path,
            "out_path": out_path,
            "voice_clone_intent": intent,
            "voice_clone_outcome": None,
            "used_voice_preserving": False,
            "error": None,
        }

        ref_path = pick_reference(seg)
        if intent != "clone" or not ref_path:
            # CosyVoice2 is a clone-only backend in this pipeline. Without a usable
            # reference there is nothing to preserve; surface it as a real failure
            # rather than emitting silence or a generic voice.
            rec["voice_clone_outcome"] = "failed"
            rec["error"] = (
                "no usable speaker reference for clone-intent segment"
                if intent == "clone"
                else "standard_tts not supported by the cosyvoice backend"
            )
            rec["base_exists"] = False
            rec["out_exists"] = False
            segments.append(rec)
            print(f"[cosyvoice] seg {idx}: FAILED ({rec['error']})", flush=True)
            continue

        chunks = []
        try:
            dur = reference_duration_seconds(ref_path)
            if dur is not None and dur > MAX_REFERENCE_SECONDS:
                raise RuntimeError(
                    f"reference {dur:.1f}s exceeds CosyVoice {MAX_REFERENCE_SECONDS:.0f}s limit"
                )

            # The CosyVoice frontend load_wav()s the reference itself (16k for tokens
            # + speaker embedding, 24k for features), so pass the PATH, not a tensor.
            en_text = f"<|en|>{text}"
            chunks = [
                r["tts_speech"]
                for r in cosyvoice.inference_cross_lingual(en_text, ref_path, stream=False)
                if r is not None and "tts_speech" in r
            ]
            if not chunks:
                raise RuntimeError("CosyVoice produced no audio")
            audio = torch.cat(chunks, dim=1)  # concatenate multi-sentence output

            os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
            save_generated_wav(out_path, audio, sample_rate)
            if not (os.path.isfile(out_path) and os.path.getsize(out_path) > 0):
                raise RuntimeError("CosyVoice wrote no output file")

            converted_ok += 1
            rec["used_voice_preserving"] = True
            rec["voice_clone_outcome"] = "converted"
            print(f"[cosyvoice] seg {idx}: converted ({audio.shape[-1]} samples)", flush=True)
        except Exception as exc:  # noqa: BLE001 - record real failure, never silence
            rec["voice_clone_outcome"] = "failed"
            rec["error"] = f"clone_failed: {exc}"
            rec["failure_diagnostics"] = failed_segment_diagnostics(seg, ref_path, chunks)
            print(f"[cosyvoice] seg {idx}: FAILED {exc}", flush=True)
            traceback.print_exc()

        rec["base_exists"] = os.path.isfile(rec["base_out_path"]) and os.path.getsize(rec["base_out_path"]) > 0
        rec["out_exists"] = os.path.isfile(out_path) and os.path.getsize(out_path) > 0
        segments.append(rec)

    if clone_requested == 0:
        run_outcome = "standard_tts_only" if standard_tts_segments > 0 else None
    elif converted_ok >= clone_requested and clone_fallback == 0:
        run_outcome = "clone_preserved"
    elif converted_ok > 0:
        run_outcome = "partial_fallback"
    else:
        run_outcome = "fallback_only"

    report = {
        "schema_version": 1,
        "created_at_ms": int(time.time() * 1000),
        "backend_id": "cosyvoice",
        "device": "cpu",
        "llm_dtype_diagnostics": dtype_diagnostics,
        "segments_total": len(segments),
        "segments_base_ok": converted_ok,
        "segments_converted_ok": converted_ok,
        "voice_clone_outcome": run_outcome,
        "voice_clone_requested_segments": clone_requested,
        "voice_clone_converted_segments": converted_ok,
        "voice_clone_fallback_segments": clone_fallback,
        "voice_clone_standard_tts_segments": standard_tts_segments,
        "segments": segments,
    }
    os.makedirs(os.path.dirname(os.path.abspath(args.report)) or ".", exist_ok=True)
    with open(args.report, "w", encoding="utf-8") as f:
        json.dump(report, f, ensure_ascii=False, indent=2)
    print(
        f"[cosyvoice] done: {run_outcome} ({converted_ok}/{clone_requested} cloned)",
        flush=True,
    )


if __name__ == "__main__":
    main()
