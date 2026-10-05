"""UNEXECUTED WP0230b proposal: explicit ephemeral owned-child instrumentation.

Only the original pinned HTTP progress context function reference is adapted. Library files,
download calls, progress visibility, return values and installation status are unchanged.
The parent must independently attest private ownership and validate original receipts.
"""
import contextlib
import hashlib
import importlib
import importlib.metadata
import itertools
import pathlib
import collections
import threading
import time

MAX_BYTES = 9007199254740991
_VV_COMMAND_STARTED_NS = time.monotonic_ns()
try:
    import ctypes
    _vv_tick_api = ctypes.WinDLL("kernel32", use_last_error=True).GetTickCount64
    _vv_tick_api.argtypes = []
    _vv_tick_api.restype = ctypes.c_ulonglong
except Exception:
    _vv_tick_api = None

def _vv_boot_tick_ms():
    try:
        return int(_vv_tick_api()) if _vv_tick_api is not None else None
    except Exception:
        return None
MODULES = ("huggingface_hub.utils.tqdm", "huggingface_hub.file_download",
           "huggingface_hub.constants", "tqdm.std", "tqdm.utils", "tqdm.auto")


def _integer(value):
    return type(value) is int and 0 <= value <= MAX_BYTES


class NonWaitingHandoff:
    """Bounded own deque; every producer/consumer lock acquisition is try-only.

    No queue.Queue, condition/event, IO callback or additional internal mutex.
    The single original HTTP thread for a bar owns its sticky loss flag.
    """
    def __init__(self, capacity=128):
        if type(capacity) is not int or not 1 <= capacity <= 128:
            raise ValueError("capacity must be 1..128")
        self._capacity = capacity
        self._lock = threading.Lock()
        self._frames = collections.deque()
        self._sequence = 0
        self.invalidated = False  # Sticky command-wide refusal; never cleared by a successful older producer.

    def try_offer(self, frame):
        if not self._lock.acquire(blocking=False):
            return False
        try:
            self._sequence += 1
            if len(self._frames) >= self._capacity:
                return False
            frame["sequence"] = self._sequence
            self._frames.append(frame)
            return True
        finally:
            self._lock.release()

    def try_take(self):
        if not self._lock.acquire(blocking=False):
            return "busy", None
        try:
            if not self._frames:
                return "empty", None
            frame = self._frames.popleft()
            if self.invalidated:
                frame["event"] = "invalid"
                frame["gap"] = True
            return "frame", frame
        finally:
            self._lock.release()


def _publish_transfer(events, bar, event, delta=None):
    """Original HTTP bar thread owns its numeric fields and sticky loss flag."""
    try:
        observed_boot_tick_ms = _vv_boot_tick_ms()  # First: later preparation/preemption counts against age.
        observed_monotonic_ns = time.monotonic_ns()
        frame = {"schema": "vv.phase2.http_transfer.v1",
                 "bar": bar._vv_id, "event": event, "unit": "B",
                 "received_bytes": bar._vv_bytes, "initial_bytes": bar._vv_initial,
                 "total_bytes": bar._vv_total, "delta_bytes": delta,
                 "observed_monotonic_ns": observed_monotonic_ns, "command_start_monotonic_ns": _VV_COMMAND_STARTED_NS, "gap": bar._vv_lost,
                 "counter_source": getattr(bar, "_vv_counter_source", "original_http_update_payload_sum"), "observed_boot_tick_ms": observed_boot_tick_ms}
        bar._vv_lost = not events.try_offer(frame)
    except Exception:
        bar._vv_lost = True


@contextlib.contextmanager
def owned_hf_progress(private_site_packages, source_pins, events):
    """Source-pinned original HTTP context adaptation, including actual Cosy aggregate forwarding.

    Neither tqdm class aliases nor aggregate reconstruction callbacks are changed.
    Unknown/mismatched effective imports keep the original downloader uninstrumented.
    """
    if type(events) is not NonWaitingHandoff:
        raise ValueError("bounded try-lock handoff required")
    try:
        root = pathlib.Path(private_site_packages).resolve(strict=True)
        versions = (importlib.metadata.version("huggingface_hub"), importlib.metadata.version("tqdm"))
        if versions not in (("0.34.4", "4.68.3"), ("1.33.0", "4.70.1")) or set(source_pins) != set(MODULES):
            raise ValueError("unsupported source version or incomplete pins")
        modules = {name: importlib.import_module(name) for name in MODULES}
        for name, module in modules.items():
            path = pathlib.Path(module.__file__).resolve(strict=True)
            if not path.is_relative_to(root) or path.suffix != ".py" or path.stat().st_size > 2097152 or hashlib.sha256(path.read_bytes()).hexdigest() != source_pins[name]:
                raise ValueError("effective module source mismatch")
        module = modules["huggingface_hub.file_download"]
        original = module._get_progress_bar_context
        if original.__module__ != "huggingface_hub.utils.tqdm" or original.__name__ != "_get_progress_bar_context":
            raise ValueError("original context reference not established")
        constants = modules["huggingface_hub.constants"]
        if not constants.HF_HUB_DISABLE_XET or getattr(constants, "HF_HUB_ENABLE_HF_TRANSFER", False):
            raise ValueError("original HTTP byte callback path not established")
    except Exception:
        yield {"available": False, "reason": "effective_pinned_http_context_not_proven"}
        return

    ids = itertools.count(1)
    active = True

    class OwnedHttpProgress:
        def __init__(self, progress, initial, total):
            self._progress = progress
            self._vv_id = next(ids)
            self._vv_initial = self._vv_bytes = initial
            self._vv_total = total if total else None
            self._vv_lost = False
            self._vv_lock = threading.Lock()
            self._vv_enabled = True
            self._vv_invalidated = False
            _publish_transfer(events, self, "begin")

        def __getattr__(self, name):
            return getattr(self._progress, name)

        def update(self, n=1):
            result = self._progress.update(n)  # Original exception/result always wins.
            if not active or not self._vv_enabled:
                return result
            if not self._vv_lock.acquire(blocking=False):
                self._vv_lost = True
                self._vv_invalidated = True
                events.invalidated = True
                self._vv_enabled = False  # Concurrent loss cannot leave a plausible partial counter.
                return result
            try:
                if self._vv_invalidated or events.invalidated:
                    _publish_transfer(events, self, "invalid")
                    return result
                if type(n) is not int or abs(n) > MAX_BYTES or not 0 <= self._vv_bytes + n <= MAX_BYTES:
                    _publish_transfer(events, self, "invalid")
                    self._vv_enabled = False
                elif self._vv_total is not None and self._vv_bytes + n > self._vv_total:
                    _publish_transfer(events, self, "invalid")
                    self._vv_enabled = False
                else:
                    self._vv_bytes += n
                    # Original Range rollback invalidates prior generation/speed, never counts reconstructed bytes.
                    if n < 0:
                        self._vv_id = next(ids)
                        self._vv_initial = self._vv_bytes
                        self._vv_lost = True
                        _publish_transfer(events, self, "begin")
                    else:
                        _publish_transfer(events, self, "update", n)
            finally:
                self._vv_lock.release()
            return result

        def update_transfer(self, n=1):
            # Cosy HTTP invokes both methods: only update above is counted, this forwards the genuine aggregate.
            original_update = getattr(self._progress, "update_transfer", None)
            return original_update(n) if callable(original_update) else None

    @contextlib.contextmanager
    def context(*args, **kwargs):
        original_cm = original(*args, **kwargs)
        with original_cm as progress:
            if isinstance(progress, OwnedHttpProgress):
                # Original shared bar remains; only telemetry generation/baseline resets.
                if progress._vv_lock.acquire(blocking=False):
                    try:
                        if progress._vv_enabled:
                            progress._vv_id = next(ids)
                            progress._vv_initial = progress._vv_bytes
                            progress._vv_lost = True
                            _publish_transfer(events, progress, "begin")
                    finally:
                        progress._vv_lock.release()
                else:
                    progress._vv_invalidated = True
                    progress._vv_enabled = False
                    events.invalidated = True
                yield progress
                return
            initial, total = kwargs.get("initial", 0), kwargs.get("total")
            if not active or kwargs.get("name") != "huggingface_hub.http_get" or kwargs.get("unit", "B") != "B" or not _integer(initial) or (total is not None and (not _integer(total) or initial > total)):
                yield progress
                return
            proxy = OwnedHttpProgress(progress, initial, total)
            try:
                yield proxy
            finally:
                proxy._vv_enabled = False
                _publish_transfer(events, proxy, "bar_closed")  # Never install/success authority.

    module._get_progress_bar_context = context
    try:
        yield {"available": True, "configuration_only": True}
    finally:
        active = False
        if module._get_progress_bar_context is context:
            module._get_progress_bar_context = original


