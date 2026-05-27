"""WP-0245 orchestrator: re-queue failed Hearin dub + Miyeon diarize, then
enqueue a fresh Miyeon dub once the diarize succeeds. Used to drive
end-to-end Localization Studio coverage (single-speaker + multi-speaker)
after the install_phase2_packs_v1 job has finished.

Safe to re-run; every step checks current state before mutating. Uses
exponential backoff on `database is locked` so direct DB writes can
coexist with the running app's writer.

Usage:
  python3 wp0245_unblock_and_retest.py wait_install
  python3 wp0245_unblock_and_retest.py retry_hearin
  python3 wp0245_unblock_and_retest.py retry_miyeon_diarize
  python3 wp0245_unblock_and_retest.py enqueue_miyeon_dub
  python3 wp0245_unblock_and_retest.py status
"""
import json
import os
import sqlite3
import sys
import time
import uuid

APPDATA = os.path.expandvars(r"%APPDATA%")
DB_PATH = APPDATA + r"\com.voxvulgi.voxvulgi\db\app.sqlite"

HEARIN_ITEM = "285097bf-b998-4b24-a390-b12e115ea580"
HEARIN_DUB_JOB = "9d1221fb-30a2-4b3f-8565-d56e6edfc961"
HEARIN_DIARIZED_TRACK = "a975cfce-6d05-4c0a-a4e0-d9bf988d4b40"

MIYEON_ITEM = "ab16785e-0fc4-4eba-9363-db81727a31db"
MIYEON_DIARIZE_JOB = "c7d3766b-20f0-41f1-8f63-76b1d4b7fb47"
MIYEON_SOURCE_TRACK = "63680350-1ef8-48df-9e1d-54f2c147637c"  # asr/source.json

INSTALL_JOB = "f450bffb-1aa0-4bb2-8ddd-dac447f62756"


def open_db(readonly=True):
    if readonly:
        return sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True, timeout=5)
    conn = sqlite3.connect(DB_PATH, timeout=30, isolation_level=None)
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA synchronous=NORMAL")
    conn.execute("PRAGMA busy_timeout=30000")
    return conn


def with_retry(fn, attempts=15, base_delay=2.0):
    last = None
    for i in range(attempts):
        try:
            return fn()
        except sqlite3.OperationalError as e:
            last = e
            msg = str(e).lower()
            if "locked" not in msg and "busy" not in msg:
                raise
            delay = base_delay * (1.4 ** i)
            print(f"  [retry {i+1}/{attempts}] DB locked, sleeping {delay:.1f}s", flush=True)
            time.sleep(delay)
    raise last


def job_status(job_id):
    conn = open_db(True)
    row = conn.execute(
        "SELECT status, progress, started_at_ms, finished_at_ms, substr(coalesce(error,''),1,300) FROM job WHERE id=?",
        (job_id,),
    ).fetchone()
    conn.close()
    return row


def now_ms():
    return int(time.time() * 1000)


def wait_install():
    print("waiting for install_phase2_packs to leave running...", flush=True)
    while True:
        row = job_status(INSTALL_JOB)
        if not row:
            print("install job not found")
            return
        status, prog, started, finished, err = row
        elapsed = (finished or now_ms()) - (started or now_ms())
        print(f"  status={status} prog={prog} elapsed={elapsed//1000}s", flush=True)
        if status not in ("running", "queued"):
            print(f"install terminal: {status}; err: {err[:200] if err else ''}")
            return status
        time.sleep(30)


def retry_job(job_id, label):
    cur = job_status(job_id)
    if not cur:
        print(f"{label}: job {job_id} not found")
        return False
    status = cur[0]
    if status == "running":
        print(f"{label}: already running, skipping retry")
        return True
    if status == "succeeded":
        print(f"{label}: already succeeded, skipping retry")
        return True

    def do_update():
        conn = open_db(False)
        conn.execute(
            "UPDATE job SET status='queued', progress=0, started_at_ms=NULL, finished_at_ms=NULL, error=NULL WHERE id=?",
            (job_id,),
        )
        conn.close()

    with_retry(do_update)
    print(f"{label}: re-queued from status={status}")
    return True


def retry_hearin():
    return retry_job(HEARIN_DUB_JOB, "hearin_dub")


def retry_miyeon_diarize():
    return retry_job(MIYEON_DIARIZE_JOB, "miyeon_diarize")


def find_miyeon_diarized_track():
    """After a successful diarize_local_v1, a new subtitle_track of kind
    'translated' with created_by 'diarize:...' should appear for Miyeon.
    Return its id, or None if not present yet.
    """
    conn = open_db(True)
    row = conn.execute(
        "SELECT id, path, created_by FROM subtitle_track WHERE item_id=? AND created_by LIKE 'diarize:%' ORDER BY rowid DESC LIMIT 1",
        (MIYEON_ITEM,),
    ).fetchone()
    conn.close()
    if row:
        return row[0]
    return None


def enqueue_miyeon_dub():
    """Insert a new dub_voice_preserving_v1 job for Miyeon with multi-speaker
    config (range 2-4 speakers, matching the WP-0235 acceptance sample)."""
    diarized = find_miyeon_diarized_track()
    if not diarized:
        print("miyeon_dub: no diarized track yet; waiting until diarize succeeds")
        return None
    # Check if a dub job already exists/is running for this item.
    conn = open_db(True)
    existing = conn.execute(
        "SELECT id, status FROM job WHERE item_id=? AND type='dub_voice_preserving_v1' ORDER BY created_at_ms DESC LIMIT 1",
        (MIYEON_ITEM,),
    ).fetchone()
    conn.close()
    if existing and existing[1] in ("queued", "running", "succeeded"):
        print(f"miyeon_dub: existing job {existing[0][:8]} status={existing[1]}; not enqueueing another")
        return existing[0]

    new_id = str(uuid.uuid4())
    params = {
        "item_id": MIYEON_ITEM,
        "source_track_id": diarized,
        "batch_on_import": False,
        "pipeline": {
            "auto_pipeline": True,
            "output_mode": "dub",
            "source_track_id": diarized,
            "separation_backend": None,
            "queue_export_pack": False,
            "queue_qc": False,
            "variant_label": "wp0245_miyeon_multi",
            "tts_backend_id": "openvoice_v2",
            "speaker_overrides": [],
            "diarization_speaker_count": {
                "mode": "range",
                "min_speakers": 2,
                "max_speakers": 4,
            },
        },
    }

    def do_insert():
        conn = open_db(False)
        conn.execute(
            "INSERT INTO job (id, item_id, batch_id, type, status, progress, error, params_json, created_at_ms, started_at_ms, finished_at_ms, logs_path) "
            "VALUES (?, ?, NULL, 'dub_voice_preserving_v1', 'queued', 0, NULL, ?, ?, NULL, NULL, '')",
            (new_id, MIYEON_ITEM, json.dumps(params), now_ms()),
        )
        conn.close()

    with_retry(do_insert)
    print(f"miyeon_dub: enqueued new job {new_id}")
    return new_id


def status():
    rows = [
        ("install", INSTALL_JOB),
        ("hearin_dub", HEARIN_DUB_JOB),
        ("miyeon_diarize", MIYEON_DIARIZE_JOB),
    ]
    for label, jid in rows:
        s = job_status(jid)
        if not s:
            print(f"{label} ({jid[:8]}): MISSING")
            continue
        st, prog, started, finished, err = s
        elapsed = ""
        if started:
            end = finished or now_ms()
            elapsed = f" elapsed={(end-started)//1000}s"
        print(f"{label} ({jid[:8]}): {st} prog={prog}{elapsed}")
        if err:
            print(f"  err: {err[:200]}")
    # Miyeon dub state
    conn = open_db(True)
    row = conn.execute(
        "SELECT id, status, progress, substr(coalesce(error,''),1,200) FROM job WHERE item_id=? AND type='dub_voice_preserving_v1' ORDER BY created_at_ms DESC LIMIT 1",
        (MIYEON_ITEM,),
    ).fetchone()
    conn.close()
    if row:
        print(f"miyeon_dub ({row[0][:8]}): {row[1]} prog={row[2]}")
        if row[3]:
            print(f"  err: {row[3][:200]}")
    else:
        print("miyeon_dub: none enqueued yet")
    # Queue paused
    conn = open_db(True)
    paused = conn.execute("SELECT value FROM meta WHERE key='jobs_queue_paused'").fetchone()
    conn.close()
    print(f"queue_paused: {paused[0] if paused else 'NULL'}")


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    cmd = sys.argv[1] if len(sys.argv) > 1 else "status"
    fn = {
        "wait_install": wait_install,
        "retry_hearin": retry_hearin,
        "retry_miyeon_diarize": retry_miyeon_diarize,
        "enqueue_miyeon_dub": enqueue_miyeon_dub,
        "status": status,
    }.get(cmd)
    if not fn:
        print(f"unknown cmd: {cmd}", file=sys.stderr)
        sys.exit(2)
    result = fn()
    if result is not None:
        print(f"result: {result}")
