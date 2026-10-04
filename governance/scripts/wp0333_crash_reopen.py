"""Root-run disposable actual-AppDatabase crash controller; never opens user DB.

Requires a completed standalone canonical backup and an already-built example.
Independent Python SQLite read-only reopening must retain every stdout ACK.
Process crash only: not power-failure or live acceptance. production_owner
exercises the runtime-owned maintenance lifecycle, without proving its cadence.
Research: https://sqlite.org/wal.html (commit end-mark and retained WAL);
https://sqlite.org/pragma.html#pragma_synchronous (FULL vs NORMAL durability).
"""
import argparse
from contextlib import closing
import hashlib
import json
import os
from pathlib import Path
import queue
import shutil
import sqlite3
import subprocess
import threading


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def guard(path):
    candidate = os.path.normcase(str(path.resolve()))
    for variable in ("APPDATA", "LOCALAPPDATA"):
        protected = os.path.normcase(str((Path(os.environ[variable]) / "com.voxvulgi.voxvulgi").resolve()))
        if candidate == protected or candidate.startswith(protected + os.sep) or protected.startswith(candidate + os.sep):
            raise RuntimeError("Production path or ancestor refused")


def readonly(path, immutable=False):
    return sqlite3.connect(path.as_uri() + "?mode=ro" + ("&immutable=1" if immutable else ""), uri=True, timeout=3)


def protected_digest(connection):
    # Every table/row is protected, except this test's exact meta-key prefix.
    result = {}
    tables = connection.execute("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name").fetchall()
    for (table,) in tables:
        quoted = '"' + table.replace('"', '""') + '"'
        sql = "SELECT * FROM " + quoted
        if table == "meta":
            sql += " WHERE key NOT GLOB 'wp0333_crash_*'"
        hashes = []
        for row in connection.execute(sql):
            encoded = json.dumps([{"blob": value.hex()} if isinstance(value, bytes) else value for value in row], ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode()
            hashes.append(hashlib.sha256(encoded).digest())
        digest = hashlib.sha256()
        for row_hash in sorted(hashes):
            digest.update(row_hash)
        result[table] = {"count": len(hashes), "sha256": digest.hexdigest()}
    return result


def write_json(path, value):
    with path.open("w", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2)
        stream.flush()
        os.fsync(stream.fileno())


def normalized(path):
    return os.path.normcase(str(Path(path).resolve())).removeprefix("\\\\?\\")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--example", type=Path, required=True)
    parser.add_argument("--source-backup", type=Path, required=True)
    parser.add_argument("--expected-source-sha256", required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--mode", choices=("baseline", "full_no_close_no_auto", "production_owner"), required=True)
    parser.add_argument("--writes", type=int, default=32)
    parser.add_argument("--event-timeout-seconds", type=int, default=60)
    args = parser.parse_args()
    if not 1 <= args.writes <= 128 or not 1 <= args.event_timeout_seconds <= 180:
        raise RuntimeError("Invalid bounded arguments")
    source = args.source_backup.resolve(strict=True)
    example = args.example.resolve(strict=True)
    root = args.root
    if not root.is_absolute() or root.exists() or not root.parent.is_dir():
        raise RuntimeError("Root must be fresh absolute child of existing directory")
    root = root.parent.resolve(strict=True) / root.name
    guard(root)
    guard(source)
    if root.is_relative_to(source.parent):
        raise RuntimeError("Fixture must not be inside backup parent")
    for suffix in ("-wal", "-shm", "-journal"):
        if Path(str(source) + suffix).exists():
            raise RuntimeError("Source must be standalone with no sidecars")
    source_hash = sha(source)
    if len(args.expected_source_sha256) != 64 or source_hash != args.expected_source_sha256.lower():
        raise RuntimeError("Source backup differs from independently designated identity")
    with closing(readonly(source, immutable=True)) as connection:
        source_schema = connection.execute("PRAGMA user_version").fetchone()[0]
        density = connection.execute("SELECT count(*) FROM job").fetchone()[0]
        if source_schema not in (59, 60, 61) or density < 1000:
            raise RuntimeError("Canonical-density schema59..61 backup required (>=1000 jobs)")
    root.mkdir()
    (root / "db").mkdir()
    database = root / "db" / "app.sqlite"
    shutil.copyfile(source, database)
    if sha(database) != source_hash or sha(source) != source_hash:
        raise RuntimeError("Backup identity changed during guarded copy")
    write_json(root / "wp0333_crash_fixture.json", {"source": str(source), "sha256": source_hash, "schema": source_schema, "job_count": density, "example_sha256": sha(example)})
    events = queue.Queue()
    child = subprocess.Popen([str(example), str(root), args.mode, str(args.writes)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=(root / "child.stderr.log").open("w"), text=True, encoding="utf-8", creationflags=subprocess.CREATE_NO_WINDOW)
    owned_pid = child.pid
    def reader():
        for line in child.stdout:
            events.put(line)
        events.put(None)
    threading.Thread(target=reader, daemon=True).start()
    acknowledgements = []
    transcript = []
    killed = False
    production_owner_ready = False
    try:
        while True:
            line = events.get(timeout=args.event_timeout_seconds)
            if line is None:
                raise RuntimeError("Owned child exited before crash boundary; inspect stderr")
            event = json.loads(line)
            transcript.append(event)
            write_json(root / "child_events.json", transcript)
            if event["event"] == "prepared":
                if event["pid"] != owned_pid or event["mode"] != args.mode or normalized(event["root"]) != normalized(root):
                    raise RuntimeError("Owned PID/mode/fixture root mismatch")
                linked_identity = event["sqlite_identity"]
                if tuple(map(int, linked_identity[0].split("."))) < (3, 51, 3) or not linked_identity[1]:
                    raise RuntimeError("Actual child SQLite prerequisite is not fixed/identified")
                with closing(readonly(database)) as connection:
                    before = protected_digest(connection)
                    schema = connection.execute("PRAGMA user_version").fetchone()[0]
                    if schema != 61:
                        raise RuntimeError("Prepared fixture did not reach current schema61")
                write_json(root / "protected_before.json", before)
                child.stdin.write("start\n")
                child.stdin.flush()
            elif event["event"] == "production_owner_ready":
                health = event["checkpoint_maintenance"]
                if args.mode != "production_owner" or production_owner_ready or event["pid"] != owned_pid or event["mode"] != args.mode or health["enabled"] is not True or health["writer_backpressure"] or health["consecutive_errors"] != 0 or health["last_error"] is not None:
                    raise RuntimeError("Production owner initialization evidence mismatch")
                production_owner_ready = True
            elif event["event"] == "ack":
                if args.mode == "production_owner" and not production_owner_ready:
                    raise RuntimeError("ACK preceded production owner initialization")
                if event["ordinal"] != len(acknowledgements):
                    raise RuntimeError("Missing/duplicate acknowledgement ordinal")
                expected_policy = (1, 1000, False) if args.mode == "baseline" else (2, 0, True)
                actual_policy = (event["synchronous"], event["wal_autocheckpoint"], event["no_checkpoint_on_close"])
                ordinal = event["ordinal"]
                if actual_policy != expected_policy or event["key"] != f"wp0333_crash_ack_{ordinal:04}" or event["payload"] != f"canonical-commit-{ordinal:04}-" + "x" * 8192:
                    raise RuntimeError("ACK policy/key/payload does not match authorized fixture")
                acknowledgements.append(event)
                write_json(root / "acknowledgements.json", acknowledgements)
            elif event["event"] == "crash_ready":
                if args.mode == "production_owner" and (not production_owner_ready or event["runtime"]["checkpoint_maintenance"]["enabled"] is not True):
                    raise RuntimeError("Production owner absent at crash boundary")
                wal = Path(str(database) + "-wal")
                if event["pid"] != owned_pid or event["acknowledged"] != args.writes or len(acknowledgements) != args.writes or event["pending_transaction_open"] is not True or not wal.is_file() or wal.stat().st_size <= 32:
                    raise RuntimeError("Crash boundary/WAL/ACK proof incomplete")
                wal_bytes = wal.stat().st_size
                child.kill()  # Only the exact Popen child started by this invocation.
                exit_code = child.wait(timeout=10)
                killed = True
                if exit_code == 0:
                    raise RuntimeError("Expected nonzero abrupt process termination")
                break
        # WAL-aware independent reopening: immutable=1 is deliberately forbidden.
        with closing(readonly(database)) as connection:
            quick_check = connection.execute("PRAGMA quick_check").fetchall()
            reopened_schema = connection.execute("PRAGMA user_version").fetchone()[0]
            after = protected_digest(connection)
            actual = dict(connection.execute("SELECT key,value FROM meta WHERE key GLOB 'wp0333_crash_*'"))
        expected = {event["key"]: event["payload"] for event in acknowledgements}
        source_unchanged = sha(source) == source_hash and all(not Path(str(source) + suffix).exists() for suffix in ("-wal", "-shm", "-journal"))
        passed = quick_check == [("ok",)] and reopened_schema == schema == 61 and after == before and actual == expected and "wp0333_crash_uncommitted" not in actual and source_unchanged
        write_json(root / "protected_after.json", after)
        write_json(root / "verdict.json", {"pass": passed, "owned_pid": owned_pid, "abrupt_exit_code": exit_code, "mode": args.mode, "production_owner_started": production_owner_ready, "acknowledgements": len(expected), "canonical_reopened_rows": len(actual), "exact_payloads_match": actual == expected, "uncommitted_absent": "wp0333_crash_uncommitted" not in actual, "protected_tables_equal": after == before, "quick_check": quick_check, "source_unchanged": source_unchanged, "source_sha256": source_hash, "source_job_count": density, "schema_after_startup": schema, "schema_after_reopen": reopened_schema, "external_ack_ledger_flush_fsync": True, "verifier_connections_closed_before_start": True, "wal_bytes_at_crash": wal_bytes, "actual_runtime_sqlite_identity": linked_identity, "independent_verifier_sqlite": sqlite3.sqlite_version, "limits": "Owned process crash only; production_owner uses the actual runtime owner without proving sustained maintenance cadence. No power-loss, live runtime or whole-WP completion claim."})
        if not passed:
            raise RuntimeError("Crash/reopen canonical proof failed")
        print(root / "verdict.json")
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=10)
        if not killed:
            write_json(root / "incomplete.json", {"pass": False, "owned_pid": owned_pid, "reason": "Did not reach proven armed crash boundary"})


if __name__ == "__main__":
    main()
