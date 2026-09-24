"""WP-0323 reproduction: do short-lived WAL connections make read-only readers see
"database is locked"? Scratch database only; never touches the operator's data.

Scenario A (app-like): every write and every read opens its own connection and closes it.
Scenario B (fix candidate): same, plus one "keeper" connection held open for the whole run,
so no close is ever the *last* close (no checkpoint-and-delete-WAL on close).
Each scenario runs a writer thread (short IMMEDIATE transactions, like the dispatch claim)
and several reader threads (read-only + query_only + busy_timeout 4000 ms, like
db::open_readonly_raw). We count reads that fail with "database is locked" and the slowest read.
"""
import os
import sqlite3
import tempfile
import threading
import time

DURATION_S = 20
READERS = 4
ROWS = 200_000


def make_db(path):
    c = sqlite3.connect(path)
    c.execute("pragma journal_mode=wal")
    c.execute("create table job(id integer primary key, status text, track text, payload text)")
    c.executemany(
        "insert into job(status, track, payload) values(?,?,?)",
        (("queued" if i % 7 else "failed", f"t{i % 9}", "x" * 200) for i in range(ROWS)),
    )
    c.commit()
    c.close()


def writer(path, stop, stats):
    while not stop.is_set():
        c = sqlite3.connect(path, timeout=10, isolation_level=None)
        c.execute("pragma journal_mode=wal")
        c.execute("pragma synchronous=normal")
        c.execute("begin immediate")
        c.execute("update job set status='running' where id=(abs(random()) % ?)+1", (ROWS,))
        c.executemany("update job set payload=? where id=?", (("y" * 200, (i * 97) % ROWS + 1) for i in range(400)))
        c.execute("commit")
        c.close()  # short-lived, like a per-operation write context
        stats["writes"] += 1
        time.sleep(0.02)


def reader(path, stop, stats, lock):
    uri = f"file:{path}?mode=ro"
    while not stop.is_set():
        t = time.time()
        try:
            c = sqlite3.connect(uri, uri=True, timeout=4.0)
            c.execute("pragma query_only=1")
            c.execute("select track, status, count(*) from job group by 1, 2").fetchall()
            c.close()
            ok = True
        except sqlite3.OperationalError as e:
            ok = False
            err = str(e)
        dt = time.time() - t
        with lock:
            stats["reads"] += 1
            stats["max_read_s"] = max(stats["max_read_s"], dt)
            if not ok:
                stats["locked"] += 1
                stats["errors"].add(err)


def run(label, keeper):
    d = tempfile.mkdtemp()
    path = os.path.join(d, "repro.sqlite")
    make_db(path)
    keep = None
    if keeper:
        keep = sqlite3.connect(path)
        keep.execute("select 1").fetchall()
    stop = threading.Event()
    lock = threading.Lock()
    stats = {"writes": 0, "reads": 0, "locked": 0, "max_read_s": 0.0, "errors": set()}
    threads = [threading.Thread(target=writer, args=(path, stop, stats))]
    threads += [threading.Thread(target=reader, args=(path, stop, stats, lock)) for _ in range(READERS)]
    for t in threads:
        t.start()
    time.sleep(DURATION_S)
    stop.set()
    for t in threads:
        t.join()
    if keep:
        keep.close()
    print(
        f"{label}: writes={stats['writes']} reads={stats['reads']} "
        f"locked={stats['locked']} max_read={stats['max_read_s']:.2f}s errors={sorted(stats['errors'])}"
    )


if __name__ == "__main__":
    print("sqlite", sqlite3.sqlite_version)
    run("A app-like (all connections short-lived)", keeper=False)
    run("B with one keeper connection held open", keeper=True)
