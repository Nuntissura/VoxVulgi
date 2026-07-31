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
import json
import os
import sys
import threading
import time
import traceback

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
            torchaudio.save(out_path, audio, sample_rate)
            if not (os.path.isfile(out_path) and os.path.getsize(out_path) > 0):
                raise RuntimeError("CosyVoice wrote no output file")

            converted_ok += 1
            rec["used_voice_preserving"] = True
            rec["voice_clone_outcome"] = "converted"
            print(f"[cosyvoice] seg {idx}: converted ({audio.shape[-1]} samples)", flush=True)
        except Exception as exc:  # noqa: BLE001 - record real failure, never silence
            rec["voice_clone_outcome"] = "failed"
            rec["error"] = f"clone_failed: {exc}"
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
